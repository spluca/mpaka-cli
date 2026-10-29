use std::{
    fmt,
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

pub struct SessionStore {
    path: PathBuf,
}

pub struct Session {
    token: String,
}

#[derive(Debug)]
pub enum SessionError {
    InvalidToken,
    MissingHome,
    Io(io::Error),
    InsecurePermissions(PathBuf),
}

impl fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidToken => write!(formatter, "session token is empty or malformed"),
            Self::MissingHome => write!(
                formatter,
                "could not resolve the user configuration directory"
            ),
            Self::Io(error) => write!(formatter, "session storage failed: {error}"),
            Self::InsecurePermissions(path) => {
                write!(
                    formatter,
                    "session file has insecure permissions: {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for SessionError {}

impl From<io::Error> for SessionError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl SessionStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn from_environment() -> Result<Self, SessionError> {
        let configured = std::env::var_os("RUSTBOX_SESSION_FILE");
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let path = Self::path_from_values(configured.as_deref().map(Path::new), home.as_deref());
        if path.as_os_str().is_empty() {
            return Err(SessionError::MissingHome);
        }
        Ok(Self::new(path))
    }

    fn path_from_values(configured: Option<&Path>, home: Option<&Path>) -> PathBuf {
        configured
            .map(Path::to_path_buf)
            .or_else(|| home.map(|path| path.join(".config/mpaka/session")))
            .unwrap_or_default()
    }

    pub fn save(&self, token: &str) -> Result<(), SessionError> {
        let session = Session::from_token(token);
        if session.token.is_empty() || session.token.contains(['\r', '\n']) {
            return Err(SessionError::InvalidToken);
        }

        if let Some(parent) = self.path.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent)?;
            #[cfg(unix)]
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        }
        if self.path.exists() {
            ensure_secure(&self.path)?;
        }

        let temporary = self
            .path
            .with_extension(format!("{}.tmp", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let result = (|| -> Result<(), SessionError> {
            let mut file = options.open(&temporary)?;
            use std::io::Write;
            file.write_all(session.token.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    #[allow(dead_code)]
    pub fn load(&self) -> Result<Session, SessionError> {
        ensure_secure(&self.path)?;
        let token = read_token(&self.path)?;
        let token = token.trim_end_matches(['\r', '\n']);
        if token.is_empty() || token.contains(['\r', '\n']) {
            return Err(SessionError::InvalidToken);
        }
        Ok(Session::from_token(token))
    }

    pub fn remove(&self) -> Result<(), SessionError> {
        ensure_secure(&self.path)?;
        fs::remove_file(&self.path)?;
        Ok(())
    }
}

impl Session {
    pub(crate) fn from_token(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }

    pub fn token(&self) -> &str {
        &self.token
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Session")
            .field("token", &"[redacted]")
            .finish()
    }
}

fn ensure_secure(path: &Path) -> Result<(), SessionError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(SessionError::InsecurePermissions(path.to_path_buf()));
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(SessionError::InsecurePermissions(path.to_path_buf()));
    }
    let _ = metadata;
    Ok(())
}

fn read_token(path: &Path) -> Result<String, SessionError> {
    #[cfg(unix)]
    {
        let mut options = OpenOptions::new();
        options.read(true).custom_flags(libc::O_NOFOLLOW);
        let mut file = options.open(path)?;
        let metadata = file.metadata()?;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(SessionError::InsecurePermissions(path.to_path_buf()));
        }
        let mut token = String::new();
        use std::io::Read;
        file.read_to_string(&mut token)?;
        Ok(token)
    }

    #[cfg(not(unix))]
    Ok(fs::read_to_string(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_explicit_session_file_when_configured() {
        let path = Path::new("/tmp/mpaka-session");
        assert_eq!(SessionStore::path_from_values(Some(path), None), path);
    }

    #[test]
    fn defaults_to_a_mpaka_config_file_under_home() {
        let home = Path::new("/home/tester");
        assert_eq!(
            SessionStore::path_from_values(None, Some(home)),
            Path::new("/home/tester/.config/mpaka/session")
        );
    }

    #[test]
    fn stores_and_loads_an_opaque_token() {
        let path = unique_test_path("round-trip");
        let store = SessionStore::new(path.clone());
        store.save("opaque-session-token").expect("save session");
        assert_eq!(
            store.load().expect("load session").token(),
            "opaque-session-token"
        );
        std::fs::remove_file(path).expect("remove session");
    }

    #[test]
    fn removes_a_stored_session() {
        let path = unique_test_path("remove");
        let store = SessionStore::new(path.clone());
        store.save("opaque-session-token").expect("save session");
        store.remove().expect("remove session");
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_symlinked_session_path() {
        let target = unique_test_path("symlink-target");
        let path = unique_test_path("symlink");
        std::fs::write(&target, "opaque-session-token").expect("write target");
        std::os::unix::fs::symlink(&target, &path).expect("create symlink");
        let store = SessionStore::new(path.clone());
        assert!(matches!(
            store.load(),
            Err(SessionError::InsecurePermissions(_))
        ));
        assert!(matches!(
            store.remove(),
            Err(SessionError::InsecurePermissions(_))
        ));
        std::fs::remove_file(path).expect("remove symlink");
        std::fs::remove_file(target).expect("remove target");
    }

    #[test]
    fn rejects_insecure_session_file_permissions() {
        let path = unique_test_path("permissions");
        let store = SessionStore::new(path.clone());
        store.save("opaque-session-token").expect("save session");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
                .expect("make session readable");
            assert!(store.load().is_err());
        }
        std::fs::remove_file(path).expect("remove session");
    }

    #[test]
    fn redacts_the_token_from_debug_output() {
        let session = Session::from_token("opaque-session-token");
        let debug = format!("{session:?}");
        assert!(!debug.contains("opaque-session-token"));
        assert!(debug.contains("redacted"));
    }

    fn unique_test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mpaka-{name}-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ))
    }
}

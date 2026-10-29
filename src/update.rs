use crate::distribution::{
    DistributionError, RELEASE_BASE_URL, ReleaseManifest, ReleaseTarget, release_artifact_url,
    release_base_url, verify_artifact_hash, verify_manifest_signature,
};
use flate2::read::GzDecoder;
use reqwest::Url;
use std::{
    cmp::Ordering,
    fmt,
    fs::{self, OpenOptions},
    io::{self, Cursor},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_RELEASE_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct UpdateRequest {
    pub version: Option<String>,
    pub check_only: bool,
    pub executable: Option<PathBuf>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum UpdateDecision {
    Apply(String),
    Noop(String),
}

#[derive(Debug, Eq, PartialEq)]
pub struct UpdateResult {
    pub current_version: String,
    pub candidate_version: String,
    pub applied: bool,
}

#[derive(Debug)]
pub enum UpdateError {
    InvalidVersion(String),
    Distribution(DistributionError),
    Request(String),
    InvalidResponse,
    TargetMismatch(String),
    Io(io::Error),
    Replacement(String),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidVersion(version) => {
                write!(formatter, "invalid release version: {version}")
            }
            Self::Distribution(error) => error.fmt(formatter),
            Self::Request(error) => write!(formatter, "release download failed: {error}"),
            Self::InvalidResponse => {
                formatter.write_str("release server returned an invalid response")
            }
            Self::TargetMismatch(target) => {
                write!(formatter, "release has no artifact for target {target}")
            }
            Self::Io(error) => write!(formatter, "update storage failed: {error}"),
            Self::Replacement(error) => write!(
                formatter,
                "could not replace the current executable: {error}"
            ),
        }
    }
}

impl std::error::Error for UpdateError {}

impl From<DistributionError> for UpdateError {
    fn from(error: DistributionError) -> Self {
        Self::Distribution(error)
    }
}

impl From<io::Error> for UpdateError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn compare_versions(current: &str, candidate: &str) -> Result<Ordering, UpdateError> {
    Ok(parse_version(current)?.cmp(&parse_version(candidate)?))
}

pub fn select_version(
    current: &str,
    explicit: Option<&str>,
    latest: &str,
) -> Result<UpdateDecision, UpdateError> {
    let candidate = explicit.unwrap_or(latest);
    parse_version(current)?;
    parse_version(candidate)?;
    if explicit.is_none() && compare_versions(current, candidate)? != Ordering::Less {
        return Ok(UpdateDecision::Noop(current.to_owned()));
    }
    Ok(UpdateDecision::Apply(candidate.to_owned()))
}

fn parse_version(version: &str) -> Result<(u64, u64, u64), UpdateError> {
    let mut parts = version
        .strip_prefix('v')
        .ok_or_else(|| UpdateError::InvalidVersion(version.to_owned()))?
        .split('.');
    let parsed = (
        parts.next().and_then(|part| part.parse().ok()),
        parts.next().and_then(|part| part.parse().ok()),
        parts.next().and_then(|part| part.parse().ok()),
    );
    if parts.next().is_some() || parsed.0.is_none() || parsed.1.is_none() || parsed.2.is_none() {
        return Err(UpdateError::InvalidVersion(version.to_owned()));
    }
    Ok((
        parsed.0.unwrap_or_default(),
        parsed.1.unwrap_or_default(),
        parsed.2.unwrap_or_default(),
    ))
}

pub fn current_executable() -> Result<PathBuf, UpdateError> {
    std::env::current_exe().map_err(UpdateError::Io)
}

#[cfg(not(windows))]
pub fn replace_executable_atomically(
    current: &Path,
    replacement: &Path,
) -> Result<(), UpdateError> {
    if !replacement.is_file() {
        return Err(UpdateError::Replacement(
            "verified replacement is missing".into(),
        ));
    }
    if current.symlink_metadata()?.file_type().is_symlink() {
        return Err(UpdateError::Replacement(
            "refusing to replace a symlinked executable".into(),
        ));
    }
    let parent = current
        .parent()
        .ok_or_else(|| UpdateError::Replacement("current executable has no parent".into()))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| UpdateError::Replacement(error.to_string()))?
        .as_nanos();
    let temporary = parent.join(format!(".mpaka-update-{}-{nonce}.tmp", std::process::id()));
    let result = (|| -> Result<(), UpdateError> {
        let mut source = fs::File::open(replacement)?;
        let mut destination = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        io::copy(&mut source, &mut destination)?;
        destination.sync_all()?;
        drop(destination);
        fs::rename(&temporary, current)
            .map_err(|error| UpdateError::Replacement(error.to_string()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
pub fn replace_executable_atomically(
    current: &Path,
    replacement: &Path,
) -> Result<(), UpdateError> {
    let helper = current_executable()?;
    let current_path = current
        .to_str()
        .ok_or_else(|| UpdateError::Replacement("current path is not UTF-8".into()))?;
    let replacement_path = replacement
        .to_str()
        .ok_or_else(|| UpdateError::Replacement("replacement path is not UTF-8".into()))?;
    std::process::Command::new(helper)
        .args([
            "__mpaka-update-helper",
            &std::process::id().to_string(),
            current_path,
            replacement_path,
        ])
        .spawn()
        .map_err(|error| UpdateError::Replacement(error.to_string()))?;
    Ok(())
}

#[cfg(windows)]
pub fn run_windows_helper(
    parent_pid: u32,
    current: &Path,
    replacement: &Path,
) -> Result<(), UpdateError> {
    loop {
        let output = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {parent_pid}"), "/NH"])
            .output()
            .map_err(|error| UpdateError::Replacement(error.to_string()))?;
        if !String::from_utf8_lossy(&output.stdout).contains(&parent_pid.to_string()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    fs::rename(replacement, current)
        .map_err(|error| UpdateError::Replacement(error.to_string()))?;
    if let Some(parent) = replacement.parent() {
        let _ = fs::remove_dir_all(parent);
    }
    Ok(())
}

pub async fn self_update(request: UpdateRequest) -> Result<UpdateResult, UpdateError> {
    let current_version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let base = release_base_url()?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| UpdateError::Request(error.to_string()))?;
    let latest = request
        .version
        .clone()
        .unwrap_or(fetch_text(&client, &base, "latest").await?);
    let decision = select_version(&current_version, request.version.as_deref(), latest.trim())?;
    let candidate_version = match decision {
        UpdateDecision::Noop(version) => version,
        UpdateDecision::Apply(version) => version,
    };
    if request.check_only || candidate_version == current_version && request.version.is_none() {
        return Ok(UpdateResult {
            current_version,
            candidate_version,
            applied: false,
        });
    }
    let target = current_target()?;
    let manifest_bytes = fetch_bytes(
        &client,
        &base,
        &format!("releases/{candidate_version}/manifest.json"),
    )
    .await?;
    let signature_bytes = fetch_bytes(
        &client,
        &base,
        &format!("releases/{candidate_version}/manifest.json.sig"),
    )
    .await?;
    verify_manifest_signature(&manifest_bytes, &signature_bytes)?;
    let manifest: ReleaseManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| UpdateError::InvalidResponse)?;
    let artifact = manifest
        .artifacts
        .into_iter()
        .find(|artifact| artifact.target == target.target_triple())
        .ok_or_else(|| UpdateError::TargetMismatch(target.to_string()))?;
    if artifact.filename != target.archive_name() {
        return Err(UpdateError::TargetMismatch(target.to_string()));
    }
    let archive_bytes = fetch_bytes(
        &client,
        &base,
        &format!("releases/{candidate_version}/{}", artifact.filename),
    )
    .await?;
    if archive_bytes.len() as u64 != artifact.size {
        return Err(UpdateError::InvalidResponse);
    }
    verify_artifact_hash(&artifact.sha256, &archive_bytes)?;
    let executable = request.executable.unwrap_or(current_executable()?);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| UpdateError::Replacement(error.to_string()))?
        .as_nanos();
    let temp_dir =
        std::env::temp_dir().join(format!("mpaka-self-update-{}-{nonce}", std::process::id()));
    fs::create_dir(&temp_dir)?;
    let replacement = temp_dir.join(target.executable_name());
    let extract_result = extract_archive(&archive_bytes, target, &replacement);
    if extract_result.is_err() {
        let _ = fs::remove_dir_all(&temp_dir);
    }
    extract_result?;
    set_executable_mode(&replacement)?;
    if request.check_only {
        let _ = fs::remove_dir_all(&temp_dir);
        return Ok(UpdateResult {
            current_version,
            candidate_version,
            applied: false,
        });
    }
    replace_executable_atomically(&executable, &replacement)?;
    #[cfg(not(windows))]
    let _ = fs::remove_dir_all(&temp_dir);
    #[cfg(windows)]
    let _keep_temp_dir = temp_dir;
    Ok(UpdateResult {
        current_version,
        candidate_version,
        applied: true,
    })
}

async fn fetch_text(
    client: &reqwest::Client,
    base: &Url,
    path: &str,
) -> Result<String, UpdateError> {
    let bytes = fetch_bytes(client, base, path).await?;
    String::from_utf8(bytes).map_err(|_| UpdateError::InvalidResponse)
}

async fn fetch_bytes(
    client: &reqwest::Client,
    base: &Url,
    path: &str,
) -> Result<Vec<u8>, UpdateError> {
    let url = base.join(path).map_err(|_| UpdateError::InvalidResponse)?;
    if url.scheme() != "https" || path.contains("..") {
        return Err(UpdateError::Distribution(
            DistributionError::InvalidReleaseUrl,
        ));
    }
    if base.as_str() == format!("{RELEASE_BASE_URL}/") && path.starts_with("releases/") {
        let mut components = path.split('/');
        let _ = components.next();
        let version = components.next().ok_or(UpdateError::InvalidResponse)?;
        let filename = components.next().ok_or(UpdateError::InvalidResponse)?;
        if components.next().is_some() {
            return Err(UpdateError::InvalidResponse);
        }
        release_artifact_url(version, filename)?;
    }
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| UpdateError::Request(error.to_string()))?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RELEASE_BYTES)
    {
        return Err(UpdateError::InvalidResponse);
    }
    if !response.status().is_success() {
        return Err(UpdateError::Request(response.status().to_string()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| UpdateError::Request(error.to_string()))?;
    if bytes.len() as u64 > MAX_RELEASE_BYTES {
        return Err(UpdateError::InvalidResponse);
    }
    Ok(bytes.to_vec())
}

fn current_target() -> Result<ReleaseTarget, UpdateError> {
    let architecture = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        "arm" => "armv7l",
        other => other,
    };
    let os = match std::env::consts::OS {
        "windows" => "Windows",
        "macos" => "Darwin",
        "linux" => "Linux",
        other => other,
    };
    ReleaseTarget::from_host(os, architecture).map_err(UpdateError::Distribution)
}

fn extract_archive(
    bytes: &[u8],
    target: ReleaseTarget,
    destination: &Path,
) -> Result<(), UpdateError> {
    if target == ReleaseTarget::X86_64PcWindowsMsvc {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| UpdateError::Replacement(error.to_string()))?;
        if archive.len() != 1
            || archive
                .by_index(0)
                .map_err(|error| UpdateError::Replacement(error.to_string()))?
                .name()
                .map_err(|error| UpdateError::Replacement(error.to_string()))?
                .as_ref()
                != target.executable_name()
        {
            return Err(UpdateError::Replacement(
                "archive contains unexpected entries".into(),
            ));
        }
        let mut entry = archive
            .by_index(0)
            .map_err(|error| UpdateError::Replacement(error.to_string()))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        io::copy(&mut entry, &mut output)?;
        output.sync_all()?;
        return Ok(());
    }
    let decoder = GzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|error| UpdateError::Replacement(error.to_string()))?;
    let mut found = false;
    for entry in entries {
        let mut entry = entry.map_err(|error| UpdateError::Replacement(error.to_string()))?;
        let path = entry
            .path()
            .map_err(|error| UpdateError::Replacement(error.to_string()))?;
        if found
            || path.to_str() != Some(target.executable_name())
            || !entry.header().entry_type().is_file()
        {
            return Err(UpdateError::Replacement(
                "archive contains unexpected entries".into(),
            ));
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        io::copy(&mut entry, &mut output)?;
        output.sync_all()?;
        found = true;
    }
    if found {
        Ok(())
    } else {
        Err(UpdateError::Replacement(
            "archive did not contain mpaka".into(),
        ))
    }
}

fn set_executable_mode(path: &Path) -> Result<(), UpdateError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    fn compares_semver_release_versions() {
        assert_eq!(
            compare_versions("v0.1.0", "v0.2.0").expect("versions"),
            Ordering::Less
        );
        assert_eq!(
            compare_versions("v1.0.0", "v1.0.0").expect("versions"),
            Ordering::Equal
        );
        assert_eq!(
            compare_versions("v2.0.0", "v1.9.9").expect("versions"),
            Ordering::Greater
        );
    }

    #[test]
    fn rejects_malformed_release_versions() {
        assert!(compare_versions("0.1.0", "v0.2.0").is_err());
        assert!(compare_versions("v0.1", "v0.2.0").is_err());
    }

    #[test]
    fn only_newer_latest_is_selected_without_explicit_rollback() {
        assert_eq!(
            select_version("v0.1.0", None, "v0.2.0").expect("new version"),
            UpdateDecision::Apply("v0.2.0".into())
        );
        assert_eq!(
            select_version("v0.2.0", None, "v0.1.0").expect("no-op"),
            UpdateDecision::Noop("v0.2.0".into())
        );
    }

    #[test]
    fn explicit_version_allows_rollback() {
        assert_eq!(
            select_version("v0.2.0", Some("v0.1.0"), "v0.2.0").expect("rollback"),
            UpdateDecision::Apply("v0.1.0".into())
        );
    }

    #[test]
    fn failed_replacement_preserves_existing_executable() {
        let current = unique_path("current");
        fs::write(&current, b"old executable").expect("current");
        let missing = current.with_extension("missing");
        assert!(replace_executable_atomically(&current, &missing).is_err());
        assert_eq!(fs::read(&current).expect("preserved"), b"old executable");
        fs::remove_file(current).expect("cleanup");
    }

    #[test]
    fn test_replacement_updates_a_non_running_file() {
        let current = unique_path("replace-current");
        let replacement = unique_path("replace-new");
        fs::write(&current, b"old executable").expect("current");
        fs::write(&replacement, b"new executable").expect("replacement");
        replace_executable_atomically(&current, &replacement).expect("replace");
        assert_eq!(fs::read(&current).expect("updated"), b"new executable");
        fs::remove_file(replacement).expect("cleanup replacement");
        fs::remove_file(current).expect("cleanup");
    }

    #[test]
    fn exposes_an_update_request_without_secrets() {
        let request = UpdateRequest {
            version: Some("v0.1.0".into()),
            check_only: true,
            executable: Some(PathBuf::from("/tmp/mpaka")),
        };
        assert_eq!(request.version.as_deref(), Some("v0.1.0"));
        assert!(request.check_only);
    }

    fn unique_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mpaka-update-{name}-{}", std::process::id()))
    }
}

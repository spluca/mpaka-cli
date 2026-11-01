use reqwest::Url;
use rsa::signature::Verifier;
use rsa::{RsaPublicKey, pkcs1v15::VerifyingKey, pkcs8::DecodePublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fmt, str::FromStr};

pub const RELEASE_BASE_URL: &str = "https://cli.mpaka.dev";

// The only key trusted for release manifests. The pre-v0.2.0 key was retired
// in v0.3.0; see docs/release.md.
pub const RELEASE_PUBLIC_KEY_PEM: &str = "-----BEGIN PUBLIC KEY-----\nMIIBojANBgkqhkiG9w0BAQEFAAOCAY8AMIIBigKCAYEAnyBHAJkNHa0jLqrilDuO\n8V5m3kgqEKuNg87kGYui6oleXj0wcxTtH7lsskSSIoTGw/rbvYmYthB7LCM6K0Fl\nJyPAawnP6SEYd0vXfU8PPpuQpgmnSKrVOtt1Xn/d/5eeu5Izgfx9qyamozyAhtKo\n6+63qgG/55KeyYJxul8uRqffedXxISg1gWCsDVigBzEJjERCks9wD0CfLCjUKdat\n8ZHL+iQZ6mdFHUmT66jLu8gEpACq+all/cvrg8SCFeQUA7Fm1XXaMvj6+gq/heLW\nFYEfcI1OBuERG3/3aZI8v1t2QGomKN0DYKM7846w4bdnEA0SXC9vpD7FPl8OYJNh\n86/W06DZ7L0X7Zh2EzCrTYT2zWcwB0zjxe7sDheTXv+qrHZKACOhoVKwMLdf1/EO\nwaPDsx9fY9QwUnZGhCpLiCFTa78l07GNchYuVlWF1ZsKgfuzMkSBhAASN+rJzMS0\nUovDuDrszsk6LdOR4mh1q+f7Bo0p0YOUfc7Erj2AEUSzAgMBAAE=\n-----END PUBLIC KEY-----\n";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReleaseTarget {
    X86_64UnknownLinuxMusl,
    Armv7UnknownLinuxMusleabihf,
    Aarch64UnknownLinuxMusl,
    X86_64PcWindowsMsvc,
    X86_64AppleDarwin,
    Aarch64AppleDarwin,
}

impl ReleaseTarget {
    pub fn from_host(os: &str, architecture: &str) -> Result<Self, DistributionError> {
        match (os, architecture) {
            ("Linux", "x86_64") => Ok(Self::X86_64UnknownLinuxMusl),
            ("Linux", "armv7l") | ("Linux", "armv7") => Ok(Self::Armv7UnknownLinuxMusleabihf),
            ("Linux", "aarch64") | ("Linux", "arm64") => Ok(Self::Aarch64UnknownLinuxMusl),
            ("Darwin", "x86_64") => Ok(Self::X86_64AppleDarwin),
            ("Darwin", "arm64") | ("Darwin", "aarch64") => Ok(Self::Aarch64AppleDarwin),
            ("Windows_NT", "AMD64" | "x86_64") | ("Windows", "AMD64" | "x86_64") => {
                Ok(Self::X86_64PcWindowsMsvc)
            }
            _ => Err(DistributionError::UnsupportedPlatform {
                os: os.to_owned(),
                architecture: architecture.to_owned(),
            }),
        }
    }

    pub const fn target_triple(self) -> &'static str {
        match self {
            Self::X86_64UnknownLinuxMusl => "x86_64-unknown-linux-musl",
            Self::Armv7UnknownLinuxMusleabihf => "armv7-unknown-linux-musleabihf",
            Self::Aarch64UnknownLinuxMusl => "aarch64-unknown-linux-musl",
            Self::X86_64PcWindowsMsvc => "x86_64-pc-windows-msvc",
            Self::X86_64AppleDarwin => "x86_64-apple-darwin",
            Self::Aarch64AppleDarwin => "aarch64-apple-darwin",
        }
    }

    pub const fn archive_name(self) -> &'static str {
        match self {
            Self::X86_64UnknownLinuxMusl => "mpaka-x86_64-unknown-linux-musl.tar.gz",
            Self::Armv7UnknownLinuxMusleabihf => "mpaka-armv7-unknown-linux-musleabihf.tar.gz",
            Self::Aarch64UnknownLinuxMusl => "mpaka-aarch64-unknown-linux-musl.tar.gz",
            Self::X86_64PcWindowsMsvc => "mpaka-x86_64-pc-windows-msvc.zip",
            Self::X86_64AppleDarwin => "mpaka-x86_64-apple-darwin.tar.gz",
            Self::Aarch64AppleDarwin => "mpaka-aarch64-apple-darwin.tar.gz",
        }
    }

    pub const fn executable_name(self) -> &'static str {
        if matches!(self, Self::X86_64PcWindowsMsvc) {
            "mpaka.exe"
        } else {
            "mpaka"
        }
    }
}

impl fmt::Display for ReleaseTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.target_triple())
    }
}

impl FromStr for ReleaseTarget {
    type Err = DistributionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "x86_64-unknown-linux-musl" => Ok(Self::X86_64UnknownLinuxMusl),
            "armv7-unknown-linux-musleabihf" => Ok(Self::Armv7UnknownLinuxMusleabihf),
            "aarch64-unknown-linux-musl" => Ok(Self::Aarch64UnknownLinuxMusl),
            "x86_64-pc-windows-msvc" => Ok(Self::X86_64PcWindowsMsvc),
            "x86_64-apple-darwin" => Ok(Self::X86_64AppleDarwin),
            "aarch64-apple-darwin" => Ok(Self::Aarch64AppleDarwin),
            _ => Err(DistributionError::UnsupportedTarget(value.to_owned())),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ReleaseManifest {
    pub version: String,
    pub artifacts: Vec<ReleaseArtifact>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ReleaseArtifact {
    pub target: String,
    pub filename: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Eq, PartialEq)]
pub enum DistributionError {
    InvalidHash(String),
    HashMismatch { expected: String, actual: String },
    InvalidSignature,
    InvalidPublicKey,
    UnsupportedPlatform { os: String, architecture: String },
    UnsupportedTarget(String),
    InvalidReleaseComponent(String),
    InvalidReleaseUrl,
}

impl fmt::Display for DistributionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHash(value) => write!(formatter, "invalid SHA-256 hash: {value}"),
            Self::HashMismatch { expected, actual } => {
                write!(
                    formatter,
                    "SHA-256 mismatch: expected {expected}, got {actual}"
                )
            }
            Self::InvalidSignature => formatter.write_str("manifest signature verification failed"),
            Self::InvalidPublicKey => formatter.write_str("embedded release public key is invalid"),
            Self::UnsupportedPlatform { os, architecture } => write!(
                formatter,
                "unsupported platform: operating system {os}, architecture {architecture}"
            ),
            Self::UnsupportedTarget(target) => {
                write!(formatter, "unsupported release target: {target}")
            }
            Self::InvalidReleaseComponent(component) => {
                write!(formatter, "invalid release path component: {component}")
            }
            Self::InvalidReleaseUrl => formatter.write_str("release URL must use HTTPS"),
        }
    }
}

impl std::error::Error for DistributionError {}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn verify_artifact_hash(expected: &str, bytes: &[u8]) -> Result<(), DistributionError> {
    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(DistributionError::InvalidHash(expected.to_owned()));
    }
    let actual = sha256_hex(bytes);
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(DistributionError::HashMismatch {
            expected: expected.to_owned(),
            actual,
        })
    }
}

pub fn verify_manifest_signature(
    manifest_bytes: &[u8],
    signature_bytes: &[u8],
) -> Result<(), DistributionError> {
    verify_manifest_signature_with_key(manifest_bytes, signature_bytes, RELEASE_PUBLIC_KEY_PEM)
}

fn verify_manifest_signature_with_key(
    manifest_bytes: &[u8],
    signature_bytes: &[u8],
    public_key_pem: &str,
) -> Result<(), DistributionError> {
    let signature = rsa::pkcs1v15::Signature::try_from(signature_bytes)
        .map_err(|_| DistributionError::InvalidSignature)?;
    let public_key = RsaPublicKey::from_public_key_pem(public_key_pem)
        .map_err(|_| DistributionError::InvalidPublicKey)?;
    VerifyingKey::<Sha256>::new(public_key)
        .verify(manifest_bytes, &signature)
        .map_err(|_| DistributionError::InvalidSignature)
}

pub fn release_artifact_url(version: &str, filename: &str) -> Result<Url, DistributionError> {
    validate_component(version)?;
    validate_component(filename)?;
    let url = Url::parse(&format!("{RELEASE_BASE_URL}/releases/{version}/{filename}"))
        .map_err(|_| DistributionError::InvalidReleaseUrl)?;
    if url.scheme() != "https" || url.host_str() != Some("cli.mpaka.dev") {
        return Err(DistributionError::InvalidReleaseUrl);
    }
    Ok(url)
}

pub fn release_base_url() -> Result<Url, DistributionError> {
    let base = std::env::var("MPAKA_CLI_BASE_URL").unwrap_or_else(|_| RELEASE_BASE_URL.to_owned());
    let base =
        Url::parse(base.trim_end_matches('/')).map_err(|_| DistributionError::InvalidReleaseUrl)?;
    if base.scheme() != "https" || base.host_str().is_none() {
        return Err(DistributionError::InvalidReleaseUrl);
    }
    Ok(base)
}

fn validate_component(component: &str) -> Result<(), DistributionError> {
    if component.is_empty()
        || component == "."
        || component == ".."
        || component.contains('/')
        || component.contains('\\')
        || component.contains('?')
        || component.contains('#')
    {
        return Err(DistributionError::InvalidReleaseComponent(
            component.to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    const VALID_MANIFEST: &[u8] = br#"{"version":"v0.1.0"}"#;
    const NEW_SIGNATURE_B64: &str = "G6IZ1TgCpB773u0lB43eYFkVD36D0BytxeOIXULO8LQyffGfofm6Rk6XoILktwU/cDEpoxX9TKR6q/zSHUS8DDXZasdjmuNQO4tV5zb3Hfp60gaCR93DiQdWxhtKZif9IlT/61y7LJMwJuwkBh/vLtV4P3C98XJTtIVE19xNBtASqV0LCRf2uqRG6q+JbYXCGxRUF/5pvkPCYE1amtks2tjEsy8IlcDTvavwYfwVO/R4bgBr7vbbpiuRbYS6NakeMdo39VzMJ/X9j1DSbimNkNy3AMrAJXuEMMW4l/jwCCRPHW0VKelMqEmlTNlWyYUCktuOpnJQjLa9ngKRawqemXtiIEN2wGnZbRPU8gGZM1p8Zt6a8IWQBdZ5RciGNTk+VL8DgehYeP/4fSXbqlR7ZarMF2znXibYXpWczARlnZi0ptu6W14gLj292abtmQJ14+Vp1oRpsyeOOBuBX9Qyem6bkA8GE2ynBMTpR0a/PqUjKuZWJCbB+ukUvQGvyN7R";

    #[test]
    fn maps_supported_hosts_to_release_targets() {
        assert_eq!(
            ReleaseTarget::from_host("Linux", "x86_64").expect("linux x64"),
            ReleaseTarget::X86_64UnknownLinuxMusl
        );
        assert_eq!(
            ReleaseTarget::from_host("Linux", "aarch64").expect("linux arm64"),
            ReleaseTarget::Aarch64UnknownLinuxMusl
        );
        assert_eq!(
            ReleaseTarget::from_host("Linux", "armv7l").expect("linux armv7"),
            ReleaseTarget::Armv7UnknownLinuxMusleabihf
        );
        assert_eq!(
            ReleaseTarget::from_host("Darwin", "x86_64").expect("mac x64"),
            ReleaseTarget::X86_64AppleDarwin
        );
        assert_eq!(
            ReleaseTarget::from_host("Darwin", "arm64").expect("mac arm64"),
            ReleaseTarget::Aarch64AppleDarwin
        );
        assert_eq!(
            ReleaseTarget::from_host("Windows_NT", "AMD64").expect("windows x64"),
            ReleaseTarget::X86_64PcWindowsMsvc
        );
        assert_eq!(
            ReleaseTarget::from_host("Windows", "x86_64").expect("windows rust target"),
            ReleaseTarget::X86_64PcWindowsMsvc
        );
    }

    #[test]
    fn rejects_unsupported_hosts_with_detected_values() {
        let error = ReleaseTarget::from_host("FreeBSD", "sparc64").expect_err("unsupported");
        assert_eq!(
            error.to_string(),
            "unsupported platform: operating system FreeBSD, architecture sparc64"
        );
    }

    #[test]
    fn renders_expected_archive_and_executable_names() {
        assert_eq!(
            ReleaseTarget::X86_64UnknownLinuxMusl.archive_name(),
            "mpaka-x86_64-unknown-linux-musl.tar.gz"
        );
        assert_eq!(
            ReleaseTarget::X86_64PcWindowsMsvc.archive_name(),
            "mpaka-x86_64-pc-windows-msvc.zip"
        );
        assert_eq!(
            ReleaseTarget::X86_64PcWindowsMsvc.executable_name(),
            "mpaka.exe"
        );
        assert_eq!(ReleaseTarget::Aarch64AppleDarwin.executable_name(), "mpaka");
    }

    #[test]
    fn deserializes_manifest_with_release_metadata() {
        let manifest: ReleaseManifest = serde_json::from_str(
            r#"{
                "version":"v0.1.0",
                "artifacts":[{"target":"x86_64-unknown-linux-musl","filename":"mpaka-x86_64-unknown-linux-musl.tar.gz","size":12,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]
            }"#,
        )
        .expect("manifest");
        assert_eq!(manifest.version, "v0.1.0");
        assert_eq!(manifest.artifacts[0].size, 12);
        assert_eq!(manifest.artifacts[0].target, "x86_64-unknown-linux-musl");
    }

    #[test]
    fn computes_and_checks_sha256() {
        let digest = sha256_hex(b"mpaka");
        assert_eq!(digest.len(), 64);
        verify_artifact_hash(&digest, b"mpaka").expect("matching hash");
        assert!(matches!(
            verify_artifact_hash(&"0".repeat(64), b"mpaka"),
            Err(DistributionError::HashMismatch { .. })
        ));
    }

    #[test]
    fn accepts_manifests_signed_by_the_release_key() {
        let signature = base64::engine::general_purpose::STANDARD
            .decode(NEW_SIGNATURE_B64)
            .expect("signature base64");
        assert!(verify_manifest_signature(VALID_MANIFEST, &signature).is_ok());
        assert!(verify_manifest_signature(b"changed", &signature).is_err());
    }

    #[test]
    fn rejects_valid_manifests_signed_by_the_retired_key() {
        let manifest = include_bytes!("../tests/fixtures/retired-key/manifest.json");
        let signature = include_bytes!("../tests/fixtures/retired-key/manifest.json.sig");
        let retired_key = include_str!("../tests/fixtures/retired-key/public.pem");
        verify_manifest_signature_with_key(manifest, signature, retired_key)
            .expect("retired-key fixture is a valid signature");

        assert!(matches!(
            verify_manifest_signature(manifest, signature),
            Err(DistributionError::InvalidSignature)
        ));
    }

    #[test]
    fn rejects_manifests_signed_by_an_untrusted_key() {
        assert!(verify_manifest_signature(VALID_MANIFEST, &[0; 384]).is_err());
    }

    #[test]
    fn builds_only_safe_release_urls() {
        let url = release_artifact_url("v0.1.0", "mpaka-x86_64-unknown-linux-musl.tar.gz")
            .expect("release URL");
        assert_eq!(
            url.as_str(),
            "https://cli.mpaka.dev/releases/v0.1.0/mpaka-x86_64-unknown-linux-musl.tar.gz"
        );
        assert!(release_artifact_url("../v0.1.0", "mpaka.tar.gz").is_err());
        assert!(release_artifact_url("v0.1.0", "../mpaka").is_err());
    }

    #[test]
    fn rejects_malformed_hashes() {
        assert!(matches!(
            verify_artifact_hash("not-a-hash", b"mpaka"),
            Err(DistributionError::InvalidHash(_))
        ));
    }
}

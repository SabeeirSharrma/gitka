use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::encryption;
use crate::error::{GitkaError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub source: SourceConfig,
    pub target: TargetConfig,
    pub repos: Vec<RepoConfig>,
    pub compression: CompressionConfig,
    pub extraction: ExtractionConfig,
    pub toggles: Toggles,
    pub encryption: Option<EncryptionConfig>,
    pub integrations: Integrations,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    /// GitHub username or organization
    pub github_username: Option<String>,
    /// GitFlare instance URL (if using GitFlare instead of GitHub)
    pub gitflare_url: Option<String>,
    /// Authentication token (GitHub PAT or GitFlare token)
    pub auth_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    /// Path to the USB/backup drive root
    pub path: PathBuf,
    /// Detection mode: "removable" (default) or "local"
    pub mode: TargetMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetMode {
    Removable,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    /// Repository name (e.g., "my-project")
    pub name: String,
    /// Whether this repo can be extracted for offline work
    pub workspace_eligible: bool,
    /// Whether to clone full history or shallow
    pub full_history: bool,
    /// Last synced commit hash
    pub last_synced: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// Compression backend: "zstd" (default) or future "sai"
    pub backend: CompressionBackend,
    /// Compression tier: "auto" (default), "low", "medium", "high"
    pub tier: CompressionTier,
    /// Dictionary size in MB (default: 32)
    pub dictionary_size_mb: u32,
    /// Volume splitting: off by default
    pub volume_splitting: Option<VolumeSplitting>,
    /// Solid archiving mode
    pub solid: SolidMode,
    /// Cross-repo deduplication
    pub dedup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionBackend {
    Zstd,
    // Reserved for future SAI backend
    // Sai,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionTier {
    Auto,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeSplitting {
    /// Split size in MB
    pub size_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolidMode {
    None,
    PerRepo,
    FullArchive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionConfig {
    /// Where to extract: "usb" (default) or "host" computer
    pub target: ExtractionTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionTarget {
    Usb,
    Host,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Toggles {
    /// Clear extraction location after lock/serve-stop
    pub clear_after_lock: bool,
    /// Verify archive integrity after sync/recompress
    pub verify_after_sync: bool,
    /// Enable AES-256-GCM encryption
    pub encryption: bool,
    /// Enable par2-style recovery records
    pub recovery_records: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionConfig {
    /// Encryption password (derived key stored separately)
    pub password: Option<String>,
    /// Salt for key derivation (hex encoded)
    pub salt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Integrations {
    /// GitFlare LAN serving config
    pub gitflare: Option<GitFlareConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitFlareConfig {
    /// Port for GitFlare server (default: 8080)
    pub port: u16,
    /// Bind address (default: "0.0.0.0")
    pub bind_address: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            source: SourceConfig {
                github_username: None,
                gitflare_url: None,
                auth_token: None,
            },
            target: TargetConfig {
                path: PathBuf::from("/"),
                mode: TargetMode::Removable,
            },
            repos: Vec::new(),
            compression: CompressionConfig {
                backend: CompressionBackend::Zstd,
                tier: CompressionTier::Auto,
                dictionary_size_mb: 32,
                volume_splitting: None,
                solid: SolidMode::PerRepo,
                dedup: true,
            },
            extraction: ExtractionConfig {
                target: ExtractionTarget::Usb,
            },
            toggles: Toggles {
                clear_after_lock: true,
                verify_after_sync: true,
                encryption: false,
                recovery_records: false,
            },
            encryption: None,
            integrations: Integrations {
                gitflare: Some(GitFlareConfig {
                    port: 8080,
                    bind_address: "0.0.0.0".to_string(),
                }),
            },
        }
    }
}

impl Config {
    /// Load config from a TOML file
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save config to a TOML file
    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get the archive directory path
    pub fn archive_dir(&self) -> PathBuf {
        self.target.path.join("repos").join("archive")
    }

    /// Get the extraction directory path
    pub fn extract_dir(&self) -> PathBuf {
        self.target.path.join("extract")
    }

    /// Get the recovery data directory path
    pub fn recovery_dir(&self) -> PathBuf {
        self.target.path.join("recovery-data")
    }

    /// Get the internal state directory path
    pub fn state_dir(&self) -> PathBuf {
        self.target.path.join(".gitka")
    }

    /// Get a repo config by name
    pub fn get_repo(&self, name: &str) -> Result<&RepoConfig> {
        self.repos
            .iter()
            .find(|r| r.name == name)
            .ok_or_else(|| GitkaError::RepoNotFound(name.to_string()))
    }

    /// Check if a repo is workspace-eligible
    pub fn is_workspace_eligible(&self, name: &str) -> Result<bool> {
        let repo = self.get_repo(name)?;
        Ok(repo.workspace_eligible)
    }

    /// Get the encryption key if encryption is enabled
    pub fn get_encryption_key(&self) -> Option<encryption::EncryptionKey> {
        if !self.toggles.encryption {
            return None;
        }

        let enc_config = self.encryption.as_ref()?;
        let password = enc_config.password.as_ref()?;
        let salt_hex = enc_config.salt.as_ref()?;

        // Decode persisted hex salt.
        let mut salt = [0u8; 16];
        let bytes = hex::decode(salt_hex).ok()?;
        if bytes.len() != 16 {
            return None;
        }
        salt.copy_from_slice(&bytes);

        Some(encryption::derive_key(password, &salt))
    }

    /// Ensure encryption has a persisted salt when encryption is enabled.
    pub fn ensure_encryption_salt(&mut self) {
        if !self.toggles.encryption {
            return;
        }

        if self.encryption.is_none() {
            self.encryption = Some(EncryptionConfig {
                password: None,
                salt: None,
            });
        }

        let enc = self.encryption.as_mut().unwrap();
        if enc.salt.is_none() {
            enc.salt = Some(hex::encode(encryption::generate_salt()));
        }
    }
}

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config::Config;
use crate::error::{GitkaError, Result};

/// The state of a repository at any given time
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RepoState {
    /// Compressed, in repos/archive/, browse/extract-only
    Archived,
    /// Temporarily decompressed for offline solo commit access
    ExtractedLocal,
    /// Temporarily decompressed and served over LAN via GitFlare
    ExtractedServed,
}

/// Metadata about a repository
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoMeta {
    /// Repository name
    pub name: String,
    /// Current state
    pub state: RepoState,
    /// Path to the archive file (relative to archive dir)
    pub archive_path: PathBuf,
    /// SHA256 hash of the archive for integrity verification
    pub archive_hash: Option<String>,
    /// Size of the archive in bytes (total across all parts)
    pub archive_size: u64,
    /// Number of volume parts (1 = no splitting)
    #[serde(default = "default_volume_count")]
    pub volume_count: u32,
    /// Names of all archive part files (relative to archive dir)
    #[serde(default)]
    pub archive_parts: Vec<String>,
    /// Size of the decompressed repo in bytes
    pub decompressed_size: Option<u64>,
    /// Last sync timestamp
    pub last_synced: Option<String>,
    /// Last verified timestamp
    pub last_verified: Option<String>,
    /// Extraction path (if extracted)
    pub extraction_path: Option<PathBuf>,
    /// Whether dedup was used for this archive
    #[serde(default)]
    pub dedup_enabled: bool,
    /// Bytes saved by dedup during last compress
    #[serde(default)]
    pub dedup_bytes_saved: u64,
}

fn default_volume_count() -> u32 {
    1
}

impl RepoMeta {
    /// Get the full path to the archive file
    pub fn archive_full_path(&self, config: &Config) -> PathBuf {
        config.archive_dir().join(&self.archive_path)
    }

    /// Check if this repo can be extracted
    pub fn can_extract(&self, config: &Config) -> Result<bool> {
        // Check if workspace-eligible
        if !config.is_workspace_eligible(&self.name)? {
            return Ok(false);
        }

        // Check if already extracted
        if self.state != RepoState::Archived {
            return Ok(false);
        }

        Ok(true)
    }

    /// Get the extraction target directory based on config
    pub fn extraction_target(&self, config: &Config) -> PathBuf {
        match config.extraction.target {
            crate::config::ExtractionTarget::Usb => config.extract_dir().join(&self.name),
            crate::config::ExtractionTarget::Host => {
                // Use system temp directory
                std::env::temp_dir().join("gitka").join(&self.name)
            }
        }
    }

    /// Get the full path to the extraction directory
    pub fn extraction_full_path(&self, config: &Config) -> PathBuf {
        if let Some(ref path) = self.extraction_path {
            path.clone()
        } else {
            self.extraction_target(config)
        }
    }
}

/// Repository state manager
pub struct RepoManager {
    config: Config,
}

impl RepoManager {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Get the metadata file path for a repo
    fn meta_path(&self, name: &str) -> PathBuf {
        self.config
            .state_dir()
            .join("repos")
            .join(format!("{}.toml", name))
    }

    /// Load metadata for a repo
    pub fn load_meta(&self, name: &str) -> Result<RepoMeta> {
        let path = self.meta_path(name);
        if !path.exists() {
            return Err(GitkaError::RepoNotFound(name.to_string()));
        }
        let content = std::fs::read_to_string(&path)?;
        let meta: RepoMeta = toml::from_str(&content)?;
        Ok(meta)
    }

    /// Save metadata for a repo
    pub fn save_meta(&self, meta: &RepoMeta) -> Result<()> {
        let path = self.meta_path(&meta.name);
        std::fs::create_dir_all(path.parent().unwrap())?;
        let content = toml::to_string_pretty(meta)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// List all repos and their states
    pub fn list_repos(&self) -> Result<Vec<RepoMeta>> {
        let state_dir = self.config.state_dir().join("repos");
        if !state_dir.exists() {
            return Ok(Vec::new());
        }

        let mut repos = Vec::new();
        for entry in std::fs::read_dir(&state_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                let content = std::fs::read_to_string(&path)?;
                let meta: RepoMeta = toml::from_str(&content)?;
                repos.push(meta);
            }
        }

        Ok(repos)
    }

    /// Check if a repo is extracted
    pub fn is_extracted(&self, name: &str) -> Result<bool> {
        let meta = self.load_meta(name)?;
        Ok(meta.state != RepoState::Archived)
    }

    /// Mark a repo as extracted
    pub fn mark_extracted(&self, name: &str, state: RepoState, extraction_path: PathBuf) -> Result<()> {
        let mut meta = self.load_meta(name)?;
        meta.state = state;
        meta.extraction_path = Some(extraction_path);
        self.save_meta(&meta)?;
        Ok(())
    }

    /// Mark a repo as archived (after recompression)
    pub fn mark_archived(&self, name: &str, archive_path: PathBuf, archive_hash: String, archive_size: u64) -> Result<()> {
        let mut meta = self.load_meta(name)?;
        meta.state = RepoState::Archived;
        meta.archive_path = archive_path;
        meta.archive_hash = Some(archive_hash);
        meta.archive_size = archive_size;
        meta.extraction_path = None;
        self.save_meta(&meta)?;
        Ok(())
    }

    /// Calculate total archive size
    pub fn total_archive_size(&self) -> Result<u64> {
        let repos = self.list_repos()?;
        Ok(repos.iter().map(|r| r.archive_size).sum())
    }

    /// Calculate total decompressed size
    pub fn total_decompressed_size(&self) -> Result<u64> {
        let repos = self.list_repos()?;
        Ok(repos.iter().filter_map(|r| r.decompressed_size).sum())
    }
}

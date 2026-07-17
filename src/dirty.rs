#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Config;
use crate::error::{GitkaError, Result};

/// Actions that can be logged
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DirtyAction {
    /// Repo was unlocked (extracted from archive)
    Unlock,
    /// Repo was served via GitFlare
    Serve,
    /// Repo was modified during session (commits made)
    Modified,
}

/// A single dirty log entry for a repo during one session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirtyEntry {
    /// Repository name
    pub repo_name: String,
    /// What action was taken
    pub action: DirtyAction,
    /// Unix timestamp when this entry was created
    pub timestamp: u64,
    /// Commit hashes created/observed during this session (audit trail)
    pub commits: Vec<String>,
    /// Files that were modified during this session (relative paths)
    pub files_touched: Vec<String>,
}

/// The full dirty log, stored as TOML
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DirtyLog {
    /// Entries keyed by repo name
    pub entries: HashMap<String, DirtyEntry>,
}

impl DirtyLog {
    /// Load the dirty log from disk
    pub fn load(config: &Config) -> Self {
        let path = dirty_log_path(config);
        if !path.exists() {
            return Self::default();
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save the dirty log to disk
    pub fn save(&self, config: &Config) -> Result<()> {
        let path = dirty_log_path(config);

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)
            .map_err(|e| GitkaError::Config(format!("Failed to serialize dirty log: {}", e)))?;

        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Check if the log is empty (clean session)
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get all dirty repo names
    pub fn dirty_repos(&self) -> Vec<&str> {
        self.entries.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a specific repo is dirty
    pub fn is_dirty(&self, repo_name: &str) -> bool {
        self.entries.contains_key(repo_name)
    }

    /// Record that a repo was unlocked
    pub fn record_unlock(&mut self, repo_name: &str) {
        let now = current_timestamp();
        self.entries.insert(
            repo_name.to_string(),
            DirtyEntry {
                repo_name: repo_name.to_string(),
                action: DirtyAction::Unlock,
                timestamp: now,
                commits: Vec::new(),
                files_touched: Vec::new(),
            },
        );
    }

    /// Record that a repo was served
    pub fn record_serve(&mut self, repo_name: &str) {
        let now = current_timestamp();
        self.entries.insert(
            repo_name.to_string(),
            DirtyEntry {
                repo_name: repo_name.to_string(),
                action: DirtyAction::Serve,
                timestamp: now,
                commits: Vec::new(),
                files_touched: Vec::new(),
            },
        );
    }

    /// Record a commit hash for a dirty repo
    pub fn record_commit(&mut self, repo_name: &str, commit_hash: &str) {
        if let Some(entry) = self.entries.get_mut(repo_name) {
            if !entry.commits.contains(&commit_hash.to_string()) {
                entry.commits.push(commit_hash.to_string());
            }
            // Update timestamp to latest activity
            entry.timestamp = current_timestamp();
        }
    }

    /// Record a file touched for a dirty repo
    pub fn record_file_touched(&mut self, repo_name: &str, file_path: &str) {
        if let Some(entry) = self.entries.get_mut(repo_name) {
            if !entry.files_touched.contains(&file_path.to_string()) {
                entry.files_touched.push(file_path.to_string());
            }
        }
    }

    /// Mark a repo as modified (commits were made)
    pub fn record_modified(&mut self, repo_name: &str) {
        if let Some(entry) = self.entries.get_mut(repo_name) {
            entry.action = DirtyAction::Modified;
            entry.timestamp = current_timestamp();
        } else {
            self.record_unlock(repo_name);
            if let Some(entry) = self.entries.get_mut(repo_name) {
                entry.action = DirtyAction::Modified;
            }
        }
    }

    /// Remove a repo from the dirty log (cleaned up after sync)
    pub fn clear_repo(&mut self, repo_name: &str) {
        self.entries.remove(repo_name);
    }

    /// Clear the entire dirty log (after successful sync)
    pub fn clear_all(&mut self) {
        self.entries.clear();
    }

    /// Detect orphaned repos: repos that appear extracted in metadata
    /// but aren't in the dirty log — suggests a crash/unclean exit
    pub fn detect_orphaned(
        &self,
        extracted_repos: &[String],
    ) -> Vec<String> {
        extracted_repos
            .iter()
            .filter(|name| !self.is_dirty(name))
            .cloned()
            .collect()
    }
}

/// Get the path to the dirty log file
fn dirty_log_path(config: &Config) -> PathBuf {
    config.state_dir().join("dirty.toml")
}

/// Get current Unix timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Check if the dirty log exists and is non-empty
pub fn has_dirty_log(config: &Config) -> bool {
    let log = DirtyLog::load(config);
    !log.is_empty()
}

/// Get a human-readable summary of the dirty log
pub fn dirty_log_summary(config: &Config) -> String {
    let log = DirtyLog::load(config);
    if log.is_empty() {
        return "No dirty repos (clean session)".to_string();
    }

    let mut lines = Vec::new();
    for (name, entry) in &log.entries {
        let age_secs = current_timestamp().saturating_sub(entry.timestamp);
        let age = format_age(age_secs);

        let mut line = format!("  {} - {:?} ({} ago)", name, entry.action, age);

        if !entry.commits.is_empty() {
            line.push_str(&format!(", {} commits", entry.commits.len()));
        }
        if !entry.files_touched.is_empty() {
            line.push_str(&format!(", {} files", entry.files_touched.len()));
        }

        lines.push(line);
    }

    format!("{} dirty repo(s):\n{}", log.entries.len(), lines.join("\n"))
}

/// Format a duration in seconds to a human-readable string
fn format_age(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    fn test_config(dir: &Path) -> Config {
        let mut config = Config::default();
        config.target.path = dir.to_path_buf();
        config
    }

    #[test]
    fn test_dirty_log_roundtrip() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());

        let mut log = DirtyLog::default();
        log.record_unlock("test-repo");
        log.record_commit("test-repo", "abc123");
        log.record_file_touched("test-repo", "src/main.rs");
        log.save(&config).unwrap();

        let loaded = DirtyLog::load(&config);
        assert!(loaded.is_dirty("test-repo"));
        assert_eq!(loaded.entries.len(), 1);

        let entry = loaded.entries.get("test-repo").unwrap();
        assert_eq!(entry.action, DirtyAction::Unlock);
        assert_eq!(entry.commits, vec!["abc123"]);
        assert_eq!(entry.files_touched, vec!["src/main.rs"]);
    }

    #[test]
    fn test_dirty_log_clear() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());

        let mut log = DirtyLog::default();
        log.record_unlock("repo-a");
        log.record_serve("repo-b");
        log.save(&config).unwrap();

        assert!(!log.is_empty());
        assert_eq!(log.dirty_repos().len(), 2);

        log.clear_repo("repo-a");
        assert_eq!(log.dirty_repos().len(), 1);
        assert!(log.is_dirty("repo-b"));

        log.clear_all();
        assert!(log.is_empty());
    }

    #[test]
    fn test_detect_orphaned() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());

        let mut log = DirtyLog::default();
        log.record_unlock("tracked-repo");
        log.save(&config).unwrap();

        let extracted = vec![
            "tracked-repo".to_string(),
            "orphaned-repo".to_string(),
        ];

        let orphaned = log.detect_orphaned(&extracted);
        assert_eq!(orphaned, vec!["orphaned-repo"]);
    }
}

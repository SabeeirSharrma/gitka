#![allow(dead_code)]

use thiserror::Error;

#[derive(Error, Debug)]
pub enum GitkaError {
    #[error("Config error: {0}")]
    Config(String),

    #[error("Repo not found: {0}")]
    RepoNotFound(String),

    #[error("Repo already extracted: {0}")]
    AlreadyExtracted(String),

    #[error("Repo not extracted: {0}")]
    NotExtracted(String),

    #[error("Not workspace-eligible: {0}")]
    NotWorkspaceEligible(String),

    #[error("Compression error: {0}")]
    Compression(String),

    #[error("Extraction error: {0}")]
    Extraction(String),

    #[error("USB detection error: {0}")]
    UsbDetection(String),

    #[error("Budget exceeded: {needed} needed, {available} available")]
    BudgetExceeded { needed: u64, available: u64 },

    #[error("Sync conflict in repo {0}: manual resolution required")]
    SyncConflict(String),

    #[error("Verification failed for repo {0}: {1}")]
    VerificationFailed(String, String),

    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Toml parse error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("Toml serialize error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
}

pub type Result<T> = std::result::Result<T, GitkaError>;

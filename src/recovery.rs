#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config::Config;
use crate::error::{GitkaError, Result};

/// Recovery record metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecoveryInfo {
    /// Repository name
    pub repo_name: String,
    /// Path to the par2 recovery files
    pub par2_dir: PathBuf,
    /// Original file size before recovery data
    pub original_size: u64,
    /// Total recovery data size
    pub recovery_size: u64,
    /// Recovery block count
    pub block_count: u32,
}

/// Check if par2 is available on the system
pub fn is_par2_available() -> bool {
    Command::new("par2")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Create recovery records for a file
pub fn create_recovery(
    file_path: &Path,
    recovery_dir: &Path,
    redundancy_percent: u32,
) -> Result<RecoveryInfo> {
    if !is_par2_available() {
        return Err(GitkaError::Config(
            "par2 not found. Install it: sudo apt install par2\n\
             Recovery records require par2."
                .to_string(),
        ));
    }

    if !file_path.exists() {
        return Err(GitkaError::Config(format!(
            "File not found: {}",
            file_path.display()
        )));
    }

    // Create recovery directory
    std::fs::create_dir_all(recovery_dir)?;

    let file_name = file_path.file_name()
        .ok_or_else(|| GitkaError::Config("Invalid file path".to_string()))?;

    // Run par2 create
    let output = Command::new("par2")
        .args(&[
            "create",
            "-r", &redundancy_percent.to_string(),
            "--",
            &recovery_dir.join(file_name).to_string_lossy(),
            &file_path.to_string_lossy(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| GitkaError::Config(format!("Failed to run par2: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitkaError::Config(format!(
            "par2 create failed: {}",
            stderr
        )));
    }

    // Get file sizes
    let original_size = std::fs::metadata(file_path)
        .map(|m| m.len())
        .unwrap_or(0);

    // Calculate total recovery size
    let recovery_size: u64 = std::fs::read_dir(recovery_dir)
        .map_err(|e| GitkaError::Config(format!("Failed to read recovery dir: {}", e)))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "par2").unwrap_or(false))
        .filter_map(|e| std::fs::metadata(e.path()).ok())
        .map(|m| m.len())
        .sum();

    // Get block count from par2 output (or estimate)
    let block_count = (redundancy_percent / 10 + 1) * 10; // rough estimate

    Ok(RecoveryInfo {
        repo_name: file_name.to_string_lossy().to_string(),
        par2_dir: recovery_dir.to_path_buf(),
        original_size,
        recovery_size,
        block_count,
    })
}

/// Verify recovery records
pub fn verify_recovery(recovery_dir: &Path) -> Result<bool> {
    if !is_par2_available() {
        return Err(GitkaError::Config(
            "par2 not found. Install it: sudo apt install par2".to_string(),
        ));
    }

    // Find the first .par2 file
    let par2_file = find_par2_file(recovery_dir)?;

    let output = Command::new("par2")
        .args(&["verify", "--", &par2_file.to_string_lossy()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| GitkaError::Config(format!("Failed to run par2 verify: {}", e)))?;

    Ok(output.status.success())
}

/// Repair a file using recovery records
pub fn repair_file(_file_path: &Path, recovery_dir: &Path) -> Result<()> {
    if !is_par2_available() {
        return Err(GitkaError::Config(
            "par2 not found. Install it: sudo apt install par2".to_string(),
        ));
    }

    if !recovery_dir.exists() {
        return Err(GitkaError::Config(format!(
            "Recovery directory not found: {}",
            recovery_dir.display()
        )));
    }

    // Find the first .par2 file
    let par2_file = find_par2_file(recovery_dir)?;

    // Run par2 repair
    let output = Command::new("par2")
        .args(&["repair", "--", &par2_file.to_string_lossy()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| GitkaError::Config(format!("Failed to run par2 repair: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitkaError::Config(format!(
            "par2 repair failed: {}",
            stderr
        )));
    }

    Ok(())
}

/// Get recovery info for a repo
pub fn get_recovery_info(config: &Config, repo_name: &str) -> Option<RecoveryInfo> {
    let recovery_dir = config.recovery_dir().join(repo_name);
    if !recovery_dir.exists() {
        return None;
    }

    // Try to load saved info
    let info_path = recovery_dir.join("recovery.json");
    if info_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&info_path) {
            if let Ok(info) = serde_json::from_str::<RecoveryInfo>(&content) {
                return Some(info);
            }
        }
    }

    // Create info from directory state
    let recovery_size: u64 = std::fs::read_dir(&recovery_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "par2").unwrap_or(false))
        .filter_map(|e| std::fs::metadata(e.path()).ok())
        .map(|m| m.len())
        .sum();

    if recovery_size == 0 {
        return None;
    }

    Some(RecoveryInfo {
        repo_name: repo_name.to_string(),
        par2_dir: recovery_dir,
        original_size: 0,
        recovery_size,
        block_count: 0,
    })
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Find the first .par2 file in a directory
fn find_par2_file(dir: &Path) -> Result<PathBuf> {
    for entry in std::fs::read_dir(dir)
        .map_err(|e| GitkaError::Config(format!("Failed to read dir: {}", e)))?
    {
        let entry = entry.map_err(|e| GitkaError::Config(format!("Read error: {}", e)))?;
        let path = entry.path();
        if path.extension().map(|ext| ext == "par2").unwrap_or(false) {
            return Ok(path);
        }
    }

    Err(GitkaError::Config(format!(
        "No .par2 files found in {}",
        dir.display()
    )))
}

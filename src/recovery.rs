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

/// Create recovery records for multiple volume parts
pub fn create_recovery_parts(
    archive_path: &Path,
    part_files: &[String],
    recovery_dir: &Path,
    redundancy_percent: u32,
) -> Result<Vec<RecoveryInfo>> {
    let mut results = Vec::new();
    let parent = archive_path.parent().unwrap_or(Path::new("."));

    let files_to_recover: Vec<std::path::PathBuf> = if part_files.len() <= 1 {
        vec![archive_path.to_path_buf()]
    } else {
        part_files.iter()
            .map(|name| parent.join(name))
            .collect()
    };

    for file_path in &files_to_recover {
        if file_path.exists() {
            let part_recovery_dir = recovery_dir.join(
                file_path.file_name()
                    .unwrap_or_default()
            );
            match create_recovery(file_path, &part_recovery_dir, redundancy_percent) {
                Ok(info) => results.push(info),
                Err(e) => {
                    eprintln!("  Warning: Failed to create recovery for {}: {}", file_path.display(), e);
                }
            }
        }
    }

    Ok(results)
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_is_par2_available() {
        // This test checks if par2 is installed on the system
        // It's not a failure if par2 is not available
        let available = is_par2_available();
        println!("par2 available: {}", available);
        // We don't assert here since par2 may not be installed
    }

    #[test]
    fn test_create_and_verify_recovery() {
        // Skip if par2 is not available
        if !is_par2_available() {
            println!("Skipping recovery test: par2 not installed");
            return;
        }

        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        let recovery_dir = temp_dir.path().join("recovery");

        // Create a test file
        std::fs::write(&test_file, b"Hello, World! This is a test file for recovery records.").unwrap();

        // Create recovery records
        let info = create_recovery(&test_file, &recovery_dir, 25).unwrap();
        assert!(info.recovery_size > 0);
        assert!(info.block_count > 0);

        // Verify recovery records
        let valid = verify_recovery(&recovery_dir).unwrap();
        assert!(valid);
    }

    #[test]
    fn test_repair_file() {
        // Skip if par2 is not available
        if !is_par2_available() {
            println!("Skipping repair test: par2 not installed");
            return;
        }

        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        let recovery_dir = temp_dir.path().join("recovery");

        // Create a test file
        let original_content = b"Hello, World! This is a test file for recovery records.";
        std::fs::write(&test_file, original_content).unwrap();

        // Create recovery records
        create_recovery(&test_file, &recovery_dir, 25).unwrap();

        // Corrupt the file (overwrite with different content)
        std::fs::write(&test_file, b"CORRUPTED CONTENT").unwrap();

        // Repair the file
        repair_file(&test_file, &recovery_dir).unwrap();

        // Verify the file is restored
        let repaired_content = std::fs::read(&test_file).unwrap();
        assert_eq!(repaired_content, original_content);
    }
}

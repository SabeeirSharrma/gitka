#![allow(dead_code)]

use std::path::{Path, PathBuf};

use crate::config::TargetMode;
use crate::error::{GitkaError, Result};

/// Information about a detected drive
#[derive(Debug, Clone)]
pub struct DriveInfo {
    /// Mount point or drive letter
    pub mount_point: PathBuf,
    /// Filesystem type
    pub fs_type: String,
    /// Total space in bytes
    pub total_space: u64,
    /// Free space in bytes
    pub free_space: u64,
    /// Whether this is a removable drive
    pub is_removable: bool,
    /// Drive label/name
    pub label: Option<String>,
}

/// Detect drives based on the target mode
pub fn detect_drives(mode: &TargetMode) -> Result<Vec<DriveInfo>> {
    match mode {
        TargetMode::Removable => detect_removable_drives(),
        TargetMode::Local => detect_local_drives(),
    }
}

/// Detect removable drives (USB/CD)
fn detect_removable_drives() -> Result<Vec<DriveInfo>> {
    #[cfg(target_os = "linux")]
    {
        detect_removable_drives_linux()
    }

    #[cfg(target_os = "windows")]
    {
        detect_removable_drives_windows()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Err(GitkaError::UsbDetection(
            "Unsupported platform for USB detection".to_string(),
        ))
    }
}

/// Detect local drives (non-removable)
fn detect_local_drives() -> Result<Vec<DriveInfo>> {
    #[cfg(target_os = "linux")]
    {
        detect_local_drives_linux()
    }

    #[cfg(target_os = "windows")]
    {
        detect_local_drives_windows()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Err(GitkaError::UsbDetection(
            "Unsupported platform for drive detection".to_string(),
        ))
    }
}

/// Linux removable drive detection via /sys/block/*/removable
#[cfg(target_os = "linux")]
fn detect_removable_drives_linux() -> Result<Vec<DriveInfo>> {
    let mut drives = Vec::new();

    // Read /sys/block to find removable drives
    let sys_block = Path::new("/sys/block");
    if !sys_block.exists() {
        return Err(GitkaError::UsbDetection(
            "/sys/block not available".to_string(),
        ));
    }

    for entry in std::fs::read_dir(sys_block)
        .map_err(|e| GitkaError::UsbDetection(format!("Failed to read /sys/block: {}", e)))?
    {
        let entry = entry.map_err(|e| GitkaError::UsbDetection(format!("Read error: {}", e)))?;
        let device_name = entry.file_name();
        let device_name_str = device_name.to_string_lossy();

        // Skip loop devices and ram devices
        if device_name_str.starts_with("loop") || device_name_str.starts_with("ram") {
            continue;
        }

        // Check if removable
        let removable_path = entry.path().join("removable");
        if removable_path.exists() {
            let removable = std::fs::read_to_string(&removable_path)
                .map_err(|e| GitkaError::UsbDetection(format!("Read removable: {}", e)))?;
            let removable = removable.trim() == "1";

            if removable {
                // Try to find the mount point
                if let Some(mount_point) = find_mount_point(&device_name_str) {
                    let (total_space, free_space) = get_disk_space(&mount_point)?;

                    drives.push(DriveInfo {
                        mount_point,
                        fs_type: "unknown".to_string(), // TODO: detect fs type
                        total_space,
                        free_space,
                        is_removable: true,
                        label: None,
                    });
                }
            }
        }
    }

    Ok(drives)
}

/// Linux local drive detection
#[cfg(target_os = "linux")]
fn detect_local_drives_linux() -> Result<Vec<DriveInfo>> {
    let mut drives = Vec::new();

    // Read /sys/block to find non-removable drives
    let sys_block = Path::new("/sys/block");
    if !sys_block.exists() {
        return Err(GitkaError::UsbDetection(
            "/sys/block not available".to_string(),
        ));
    }

    for entry in std::fs::read_dir(sys_block)
        .map_err(|e| GitkaError::UsbDetection(format!("Failed to read /sys/block: {}", e)))?
    {
        let entry = entry.map_err(|e| GitkaError::UsbDetection(format!("Read error: {}", e)))?;
        let device_name = entry.file_name();
        let device_name_str = device_name.to_string_lossy();

        // Skip loop devices and ram devices
        if device_name_str.starts_with("loop") || device_name_str.starts_with("ram") {
            continue;
        }

        // Check if NOT removable
        let removable_path = entry.path().join("removable");
        if removable_path.exists() {
            let removable = std::fs::read_to_string(&removable_path)
                .map_err(|e| GitkaError::UsbDetection(format!("Read removable: {}", e)))?;
            let removable = removable.trim() == "1";

            if !removable {
                // Try to find the mount point
                if let Some(mount_point) = find_mount_point(&device_name_str) {
                    let (total_space, free_space) = get_disk_space(&mount_point)?;

                    drives.push(DriveInfo {
                        mount_point,
                        fs_type: "unknown".to_string(),
                        total_space,
                        free_space,
                        is_removable: false,
                        label: None,
                    });
                }
            }
        }
    }

    Ok(drives)
}

/// Windows removable drive detection via WMI
#[cfg(target_os = "windows")]
fn detect_removable_drives_windows() -> Result<Vec<DriveInfo>> {
    // TODO: Implement WMI-based detection for Windows
    // For now, return empty
    Ok(Vec::new())
}

/// Windows local drive detection
#[cfg(target_os = "windows")]
fn detect_local_drives_windows() -> Result<Vec<DriveInfo>> {
    // TODO: Implement WMI-based detection for Windows
    Ok(Vec::new())
}

/// Find the mount point for a device
#[cfg(target_os = "linux")]
fn find_mount_point(device_name: &str) -> Option<PathBuf> {
    // Read /proc/mounts to find the mount point
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;

    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let device = parts[0];
            let mount_point = parts[1];

            // Check if this device matches (e.g., /dev/sdb1 matches sdb)
            if device.contains(device_name) {
                return Some(PathBuf::from(mount_point));
            }
        }
    }

    None
}

/// Get disk space for a mount point
#[cfg(target_os = "linux")]
fn get_disk_space(mount_point: &Path) -> Result<(u64, u64)> {
    let _metadata = std::fs::metadata(mount_point)
        .map_err(|e| GitkaError::UsbDetection(format!("Failed to stat {}: {}", mount_point.display(), e)))?;

    // Use statvfs for accurate space info
    // For now, return approximate values based on metadata
    // TODO: Implement proper statvfs call
    Ok((0, 0))
}

/// Validate that a path is on a removable drive
pub fn validate_removable(path: &Path) -> Result<bool> {
    // Check if the path exists and is on a removable drive
    if !path.exists() {
        return Err(GitkaError::UsbDetection(format!(
            "Path {} does not exist",
            path.display()
        )));
    }

    // For now, just check if it's writable
    // TODO: Implement proper removable drive validation
    Ok(true)
}

/// Get drive info for a specific path
pub fn get_drive_info(path: &Path) -> Result<DriveInfo> {
    #[cfg(target_os = "linux")]
    {
        get_drive_info_linux(path)
    }

    #[cfg(target_os = "windows")]
    {
        get_drive_info_windows(path)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Err(GitkaError::UsbDetection(
            "Unsupported platform".to_string(),
        ))
    }
}

/// Linux drive info for a path
#[cfg(target_os = "linux")]
fn get_drive_info_linux(path: &Path) -> Result<DriveInfo> {
    // Find which mount point contains this path
    let mounts = std::fs::read_to_string("/proc/mounts")
        .map_err(|e| GitkaError::UsbDetection(format!("Failed to read /proc/mounts: {}", e)))?;

    let mut best_mount = PathBuf::from("/");
    let mut best_len = 0;

    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let mount_point = PathBuf::from(parts[1]);
            if path.starts_with(&mount_point) && mount_point.as_os_str().len() > best_len {
                best_len = mount_point.as_os_str().len();
                best_mount = mount_point;
            }
        }
    }

    let (total_space, free_space) = get_disk_space(&best_mount)?;

    // Check if removable
    let device_name = mounts.lines()
        .find(|line| line.contains(best_mount.to_str().unwrap_or("")))
        .and_then(|line| line.split_whitespace().next())
        .map(|s| s.to_string());

    let is_removable = device_name
        .map(|d| {
            let device_file = format!("/sys/block/{}/removable", 
                d.trim_start_matches("/dev/").trim_end_matches("1"));
            std::fs::read_to_string(&device_file)
                .map(|s| s.trim() == "1")
                .unwrap_or(false)
        })
        .unwrap_or(false);

    Ok(DriveInfo {
        mount_point: best_mount,
        fs_type: "unknown".to_string(),
        total_space,
        free_space,
        is_removable,
        label: None,
    })
}

/// Windows drive info for a path
#[cfg(target_os = "windows")]
fn get_drive_info_windows(path: &Path) -> Result<DriveInfo> {
    // TODO: Implement Windows drive info
    Err(GitkaError::UsbDetection(
        "Windows drive info not yet implemented".to_string(),
    ))
}

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
        // Try lsblk first (more reliable), fall back to /sys/block
        detect_removable_drives_linux_lsblk()
            .or_else(|_| detect_removable_drives_linux_sysfs())
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
        detect_local_drives_linux_lsblk()
            .or_else(|_| detect_local_drives_linux_sysfs())
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

// ============================================================================
// Linux: lsblk-based detection (preferred, most reliable)
// ============================================================================

/// Linux removable drive detection via lsblk
#[cfg(target_os = "linux")]
fn detect_removable_drives_linux_lsblk() -> Result<Vec<DriveInfo>> {
    let output = std::process::Command::new("lsblk")
        .args(&["-J", "-o", "NAME,SIZE,FSTYPE,MOUNTPOINT,RM,LABEL,TYPE"])
        .output()
        .map_err(|e| GitkaError::UsbDetection(format!("Failed to run lsblk: {}", e)))?;

    if !output.status.success() {
        return Err(GitkaError::UsbDetection(
            "lsblk command failed".to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| GitkaError::UsbDetection(format!("Failed to parse lsblk JSON: {}", e)))?;

    let mut drives = Vec::new();

    if let Some(blockdevices) = json["blockdevices"].as_array() {
        for device in blockdevices {
            let _dev_type = device["type"].as_str().unwrap_or("");
            let is_removable = device["rm"].as_bool().unwrap_or(false);
            let mountpoint = device["mountpoint"].as_str().unwrap_or("");
            let fstype = device["fstype"].as_str().unwrap_or("unknown");
            let label = device["label"].as_str().map(|s| s.to_string());
            let _name = device["name"].as_str().unwrap_or("");

            // We only care about removable devices with mount points
            if is_removable && !mountpoint.is_empty() && (fstype == "vfat" || fstype == "ntfs" || fstype == "exfat" || fstype == "ext4" || fstype == "ext3") {
                let mount_path = PathBuf::from(mountpoint);

                // Get disk space using statvfs
                let (total_space, free_space) = get_disk_space_statvfs(&mount_path)
                    .unwrap_or((0, 0));

                drives.push(DriveInfo {
                    mount_point: mount_path,
                    fs_type: fstype.to_string(),
                    total_space,
                    free_space,
                    is_removable: true,
                    label,
                });
            }
        }
    }

    Ok(drives)
}

/// Linux local drive detection via lsblk
#[cfg(target_os = "linux")]
fn detect_local_drives_linux_lsblk() -> Result<Vec<DriveInfo>> {
    let output = std::process::Command::new("lsblk")
        .args(&["-J", "-o", "NAME,SIZE,FSTYPE,MOUNTPOINT,RM,LABEL,TYPE"])
        .output()
        .map_err(|e| GitkaError::UsbDetection(format!("Failed to run lsblk: {}", e)))?;

    if !output.status.success() {
        return Err(GitkaError::UsbDetection(
            "lsblk command failed".to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| GitkaError::UsbDetection(format!("Failed to parse lsblk JSON: {}", e)))?;

    let mut drives = Vec::new();

    if let Some(blockdevices) = json["blockdevices"].as_array() {
        for device in blockdevices {
            let is_removable = device["rm"].as_bool().unwrap_or(false);
            let mountpoint = device["mountpoint"].as_str().unwrap_or("");
            let fstype = device["fstype"].as_str().unwrap_or("unknown");
            let label = device["label"].as_str().map(|s| s.to_string());
            let dev_type = device["type"].as_str().unwrap_or("");

            // We want non-removable disks/partitions with mount points
            if !is_removable && !mountpoint.is_empty() && dev_type == "part" {
                let mount_path = PathBuf::from(mountpoint);

                let (total_space, free_space) = get_disk_space_statvfs(&mount_path)
                    .unwrap_or((0, 0));

                drives.push(DriveInfo {
                    mount_point: mount_path,
                    fs_type: fstype.to_string(),
                    total_space,
                    free_space,
                    is_removable: false,
                    label,
                });
            }
        }
    }

    Ok(drives)
}

// ============================================================================
// Linux: /sys/block fallback (when lsblk isn't available)
// ============================================================================

/// Linux removable drive detection via /sys/block/*/removable
#[cfg(target_os = "linux")]
fn detect_removable_drives_linux_sysfs() -> Result<Vec<DriveInfo>> {
    let mut drives = Vec::new();

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
                if let Some(mount_point) = find_mount_point(&device_name_str) {
                    let (total_space, free_space) = get_disk_space_statvfs(&mount_point)
                        .unwrap_or((0, 0));

                    let fs_type = find_fs_type(&mount_point);
                    let label = find_drive_label(&device_name_str);

                    drives.push(DriveInfo {
                        mount_point,
                        fs_type,
                        total_space,
                        free_space,
                        is_removable: true,
                        label,
                    });
                }
            }
        }
    }

    Ok(drives)
}

/// Linux local drive detection via /sys/block
#[cfg(target_os = "linux")]
fn detect_local_drives_linux_sysfs() -> Result<Vec<DriveInfo>> {
    let mut drives = Vec::new();

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
                if let Some(mount_point) = find_mount_point(&device_name_str) {
                    let (total_space, free_space) = get_disk_space_statvfs(&mount_point)
                        .unwrap_or((0, 0));

                    let fs_type = find_fs_type(&mount_point);
                    let label = find_drive_label(&device_name_str);

                    drives.push(DriveInfo {
                        mount_point,
                        fs_type,
                        total_space,
                        free_space,
                        is_removable: false,
                        label,
                    });
                }
            }
        }
    }

    Ok(drives)
}

// ============================================================================
// Windows detection (WMI-based, stubs for now)
// ============================================================================

/// Windows removable drive detection via WMI
#[cfg(target_os = "windows")]
fn detect_removable_drives_windows() -> Result<Vec<DriveInfo>> {
    // TODO: Implement WMI-based detection for Windows
    Ok(Vec::new())
}

/// Windows local drive detection
#[cfg(target_os = "windows")]
fn detect_local_drives_windows() -> Result<Vec<DriveInfo>> {
    // TODO: Implement WMI-based detection for Windows
    Ok(Vec::new())
}

// ============================================================================
// Linux helper functions
// ============================================================================

/// Find the mount point for a device using /proc/mounts
#[cfg(target_os = "linux")]
fn find_mount_point(device_name: &str) -> Option<PathBuf> {
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

/// Find filesystem type for a mount point from /proc/mounts
#[cfg(target_os = "linux")]
fn find_fs_type(mount_point: &Path) -> String {
    let mounts = std::fs::read_to_string("/proc/mounts").ok().unwrap_or_default();

    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let mp = parts[1];
            let fs_type = parts[2];

            if Path::new(mp) == mount_point {
                return fs_type.to_string();
            }
        }
    }

    "unknown".to_string()
}

/// Find drive label by looking at /dev/disk/by-label/
#[cfg(target_os = "linux")]
fn find_drive_label(device_name: &str) -> Option<String> {
    let by_label = Path::new("/dev/disk/by-label");
    if !by_label.exists() {
        return None;
    }

    // Get the full device path
    let dev_path = format!("/dev/{}", device_name);

    // Check each label symlink
    if let Ok(entries) = std::fs::read_dir(by_label) {
        for entry in entries.flatten() {
            if let Ok(target) = std::fs::read_link(entry.path()) {
                // Resolve the symlink target
                let target_str = target.to_string_lossy();
                if target_str.contains(device_name) || target_str == dev_path {
                    return Some(entry.file_name().to_string_lossy().to_string());
                }
            }
        }
    }

    None
}

/// Get disk space using statvfs (libc)
#[cfg(target_os = "linux")]
fn get_disk_space_statvfs(mount_point: &Path) -> Result<(u64, u64)> {
    use std::ffi::CString;

    let path = CString::new(mount_point.to_string_lossy().as_bytes().to_vec())
        .map_err(|e| GitkaError::UsbDetection(format!("Invalid path: {}", e)))?;

    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };

    let ret = unsafe { libc::statvfs(path.as_ptr(), &mut stat as *mut libc::statvfs) };

    if ret != 0 {
        return Err(GitkaError::UsbDetection(
            format!("statvfs failed for {}", mount_point.display()),
        ));
    }

    let block_size = stat.f_frsize as u64;
    let total = stat.f_blocks * block_size;
    let free = stat.f_bavail * block_size;

    Ok((total, free))
}

/// Validate that a path is on a removable drive
pub fn validate_removable(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Err(GitkaError::UsbDetection(format!(
            "Path {} does not exist",
            path.display()
        )));
    }

    #[cfg(target_os = "linux")]
    {
        // Check if path is on a removable mount
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

        // Find the device for this mount
        let device_name = mounts.lines()
            .find(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                parts.len() >= 2 && Path::new(parts[1]) == best_mount
            })
            .and_then(|line| line.split_whitespace().next())
            .map(|s| s.to_string());

        if let Some(device) = device_name {
            let device_base = device.trim_start_matches("/dev/");
            // Remove partition number to get base device
            let base_device = device_base.trim_end_matches(|c: char| c.is_ascii_digit());

            let removable_path = format!("/sys/block/{}/removable", base_device);
            if let Ok(removable) = std::fs::read_to_string(&removable_path) {
                return Ok(removable.trim() == "1");
            }
        }

        Ok(false)
    }

    #[cfg(not(target_os = "linux"))]
    {
        // For non-Linux, just check if writable
        Ok(true)
    }
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

    let (total_space, free_space) = get_disk_space_statvfs(&best_mount)?;

    // Find the device and its properties
    let device_line = mounts.lines()
        .find(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            parts.len() >= 2 && Path::new(parts[1]) == best_mount
        });

    let (device_name, fs_type) = if let Some(line) = device_line {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            (Some(parts[0].to_string()), parts[2].to_string())
        } else {
            (None, "unknown".to_string())
        }
    } else {
        (None, "unknown".to_string())
    };

    // Check if removable
    let is_removable = device_name
        .as_ref()
        .map(|d| {
            let device_file = format!("/sys/block/{}/removable",
                d.trim_start_matches("/dev/").trim_end_matches(|c: char| c.is_ascii_digit()));
            std::fs::read_to_string(&device_file)
                .map(|s| s.trim() == "1")
                .unwrap_or(false)
        })
        .unwrap_or(false);

    // Get label
    let label = device_name
        .as_ref()
        .and_then(|d| find_drive_label(d.trim_start_matches("/dev/")));

    Ok(DriveInfo {
        mount_point: best_mount,
        fs_type,
        total_space,
        free_space,
        is_removable,
        label,
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

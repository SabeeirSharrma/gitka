#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::fs;

use crate::config::{Config, GitFlareConfig};
use crate::error::{GitkaError, Result};

/// Directory name for the serve workspace
const SERVE_DIR: &str = ".gitka/serve";

/// GitFlare config template for gitka mode
const GITFLARE_CONFIG_TEMPLATE: &str = r#"[server]
host = "{bind}"
port = {port}
repos_path = "{repos_path}"

[auth]
admin_token = ""

[ssh]
enabled = false
"#;

// ============================================================================
// Public API
// ============================================================================

/// Extract a repo and start GitFlare in single-repo mode.
/// Returns the server URL and PID.
pub fn start(config: &Config, repo_name: &str) -> Result<ServeInfo> {
    // 1. Check if already running
    let pid_path = pid_file(config, repo_name);
    if pid_path.exists() {
        if let Ok(pid_str) = fs::read_to_string(&pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                if is_process_running(pid) {
                    return Err(GitkaError::Config(format!(
                        "Server already running for {} (PID {}). Use `gitka serve {} --stop` first.",
                        repo_name, pid, repo_name
                    )));
                }
            }
        }
        let _ = fs::remove_file(&pid_path);
    }

    // 2. Check Python is available
    check_python()?;

    // 3. Get repo metadata
    let repo_manager = crate::repo::RepoManager::new(config.clone());
    let meta = repo_manager.load_meta(repo_name)?;

    let archive_path = meta.archive_full_path(config);
    if !archive_path.exists() {
        return Err(GitkaError::Extraction(format!(
            "Archive not found: {}",
            archive_path.display()
        )));
    }

    // 4. Prepare serve workspace
    let serve_dir = config.target.path.join(SERVE_DIR);
    let repos_dir = serve_dir.join("repos");
    let bare_repo_dir = repos_dir.join(format!("{}.git", repo_name));

    // Clean previous serve state
    if serve_dir.exists() {
        fs::remove_dir_all(&serve_dir)
            .map_err(|e| GitkaError::Config(format!("Failed to clean serve dir: {}", e)))?;
    }
    fs::create_dir_all(&repos_dir)?;

    // 5. Extract archive to temp location
    let extract_temp = serve_dir.join("extract");
    fs::create_dir_all(&extract_temp)?;

    println!("  Extracting archive...");
    crate::compress::decompress_directory(&archive_path, &extract_temp)?;

    // 6. Convert to bare repo
    println!("  Preparing bare repo...");
    let extracted_dirs: Vec<_> = fs::read_dir(&extract_temp)
        .map_err(|e| GitkaError::Config(format!("Failed to read extract dir: {}", e)))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join(".git").exists() || e.path().ends_with(".git"))
        .collect();

    let source_repo = if let Some(dir) = extracted_dirs.first() {
        dir.path()
    } else {
        // Assume the extract itself is the repo root
        extract_temp.clone()
    };

    // Clone as bare
    let status = Command::new("git")
        .args(&["clone", "--bare"])
        .arg(&source_repo)
        .arg(&bare_repo_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
        .map_err(|e| GitkaError::Config(format!("Failed to run git clone --bare: {}", e)))?;

    if !status.success() {
        return Err(GitkaError::Config(
            "Failed to convert to bare repo".to_string(),
        ));
    }

    // 7. Generate gitflare.toml
    let gitflare_config = config.integrations.gitflare.as_ref()
        .cloned()
        .unwrap_or(GitFlareConfig {
            port: 8080,
            bind_address: "0.0.0.0".to_string(),
        });

    let config_content = GITFLARE_CONFIG_TEMPLATE
        .replace("{bind}", &gitflare_config.bind_address)
        .replace("{port}", &gitflare_config.port.to_string())
        .replace("{repos_path}", &repos_dir.to_string_lossy());

    let config_path = serve_dir.join("gitflare.toml");
    fs::write(&config_path, &config_content)?;

    // 8. Setup Python venv and install dependencies
    setup_gitflare_venv(&serve_dir)?;

    // 9. Start GitFlare
    println!("  Starting GitFlare on port {}...", gitflare_config.port);

    let gitflare_src = find_gitflare_source()?;
    let venv_python = serve_dir.join(".venv/bin/python3");

    let child = Command::new(&venv_python)
        .args(&["-m", "uvicorn", "gitflare.main:app"])
        .arg("--host")
        .arg(&gitflare_config.bind_address)
        .arg("--port")
        .arg(gitflare_config.port.to_string())
        .arg("--app-dir")
        .arg(&gitflare_src)
        .current_dir(&serve_dir)
        .env("GITFLARE_CONFIG", &config_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| GitkaError::Config(format!("Failed to start GitFlare: {}", e)))?;

    let pid = child.id();

    // Write PID file
    fs::create_dir_all(config.state_dir())?;
    fs::write(&pid_file(config, repo_name), pid.to_string())?;

    // Write serve info file (for restart/status)
    let info = ServeInfo {
        repo_name: repo_name.to_string(),
        pid,
        port: gitflare_config.port,
        serve_dir: serve_dir.clone(),
        bare_repo_dir: bare_repo_dir.clone(),
    };
    fs::write(
        serve_dir.join("serve.json"),
        serde_json::to_string_pretty(&info).unwrap_or_default(),
    )?;

    // Update repo state
    let mut meta = meta;
    meta.state = crate::repo::RepoState::ExtractedServed;
    meta.extraction_path = Some(extract_temp);
    repo_manager.save_meta(&meta)?;

    // Detach child - it runs in background
    std::mem::forget(child);

    Ok(info)
}

/// Stop a running server for a repo
pub fn stop(config: &Config, repo_name: &str) -> Result<bool> {
    let pid_path = pid_file(config, repo_name);

    if !pid_path.exists() {
        return Ok(false);
    }

    let pid_str = fs::read_to_string(&pid_path)
        .map_err(|e| GitkaError::Config(format!("Failed to read PID file: {}", e)))?;

    let pid: u32 = pid_str.trim().parse()
        .map_err(|_| GitkaError::Config("Invalid PID file".to_string()))?;

    let killed = kill_process(pid);

    // Clean up PID file
    let _ = fs::remove_file(&pid_path);

    // Clean up serve directory
    let serve_dir = config.target.path.join(SERVE_DIR);
    if serve_dir.exists() {
        // Give the process a moment to release files
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = fs::remove_dir_all(&serve_dir);
    }

    // Update repo state
    let repo_manager = crate::repo::RepoManager::new(config.clone());
    if let Ok(mut meta) = repo_manager.load_meta(repo_name) {
        if meta.state == crate::repo::RepoState::ExtractedServed {
            meta.state = crate::repo::RepoState::Archived;
            meta.extraction_path = None;
            let _ = repo_manager.save_meta(&meta);
        }
    }

    Ok(killed)
}

/// Check if a server is running for a repo
pub fn is_running(config: &Config, repo_name: &str) -> bool {
    let pid_path = pid_file(config, repo_name);
    if !pid_path.exists() {
        return false;
    }

    if let Ok(pid_str) = fs::read_to_string(&pid_path) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            return is_process_running(pid);
        }
    }

    false
}

/// Get serve info for a repo
pub fn get_info(config: &Config, _repo_name: &str) -> Option<ServeInfo> {
    let serve_dir = config.target.path.join(SERVE_DIR);
    let info_path = serve_dir.join("serve.json");

    if !info_path.exists() {
        return None;
    }

    let content = fs::read_to_string(&info_path).ok()?;
    serde_json::from_str(&content).ok()
}

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServeInfo {
    pub repo_name: String,
    pub pid: u32,
    pub port: u16,
    pub serve_dir: PathBuf,
    pub bare_repo_dir: PathBuf,
}

impl ServeInfo {
    /// Get the clone URL for LAN access
    pub fn clone_url(&self) -> String {
        format!("http://localhost:{}", self.port)
    }

    /// Get the LAN clone URL (for other machines)
    pub fn lan_url(&self) -> String {
        // Try to get local IP
        let ip = get_local_ip().unwrap_or_else(|| "localhost".to_string());
        format!("http://{}:{}", ip, self.port)
    }
}

// ============================================================================
// Internal helpers
// ============================================================================

/// PID file path for tracking running servers
fn pid_file(config: &Config, repo_name: &str) -> PathBuf {
    config.state_dir().join(format!("{}.serve.pid", repo_name))
}

/// Check that Python 3 is available
fn check_python() -> Result<()> {
    let status = Command::new("python3")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        _ => Err(GitkaError::Config(
            "Python 3 is required for GitFlare. Install it:\n\
             Ubuntu/Debian: sudo apt install python3 python3-venv\n\
             Fedora: sudo dnf install python3\n\
             macOS: brew install python3"
                .to_string(),
        )),
    }
}

/// Find the bundled GitFlare source directory
fn find_gitflare_source() -> Result<PathBuf> {
    // The gitflare-server directory is bundled next to the binary
    // or in the working directory
    let candidates = [
        PathBuf::from("gitflare-server"),
        PathBuf::from("../gitflare-server"),
        // When installed, look relative to binary
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("gitflare-server")))
            .unwrap_or_default(),
    ];

    for candidate in &candidates {
        if candidate.exists() && candidate.join("gitflare").join("main.py").exists() {
            return Ok(candidate.clone());
        }
    }

    Err(GitkaError::Config(
        "GitFlare source not found. Ensure the gitflare-server/ directory is present.".to_string(),
    ))
}

/// Setup Python venv and install GitFlare dependencies using uv
fn setup_gitflare_venv(serve_dir: &Path) -> Result<()> {
    let venv_dir = serve_dir.join(".venv");
    let gitflare_src = find_gitflare_source()?;
    let requirements = gitflare_src.join("requirements.txt");

    // Ensure uv is available (install if needed, uninstall after)
    let uv_installed_by_us = ensure_uv()?;

    // Create venv with uv
    if !venv_dir.exists() {
        println!("  Setting up Python environment...");
        let status = Command::new("uv")
            .args(&["venv"])
            .arg(&venv_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .status()
            .map_err(|e| GitkaError::Config(format!("Failed to create venv: {}", e)))?;

        if !status.success() {
            cleanup_uv_if_installed(uv_installed_by_us);
            return Err(GitkaError::Config("Failed to create Python venv".to_string()));
        }
    }

    // Install dependencies
    if !venv_dir.join("lib").join("site-packages/fastapi").exists() {
        println!("  Installing GitFlare dependencies...");
        let status = Command::new("uv")
            .args(&["pip", "install", "-q", "-r"])
            .arg(&requirements)
            .arg("--python")
            .arg(venv_dir.join("bin/python3"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .status()
            .map_err(|e| GitkaError::Config(format!("Failed to install dependencies: {}", e)))?;

        if !status.success() {
            cleanup_uv_if_installed(uv_installed_by_us);
            return Err(GitkaError::Config(
                "Failed to install GitFlare dependencies".to_string(),
            ));
        }
    }

    // Clean up uv if we installed it
    cleanup_uv_if_installed(uv_installed_by_us);

    Ok(())
}

/// Ensure uv is available, installing if needed.
/// Returns true if we installed it (caller should clean up).
fn ensure_uv() -> Result<bool> {
    // Check if uv is already available
    let has_uv = Command::new("uv")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if has_uv {
        return Ok(false);
    }

    // Install uv using the official installer
    println!("  Installing uv...");
    let status = Command::new("curl")
        .args(&["-LsSf", "https://astral.sh/uv/install.sh"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| GitkaError::Config(format!("Failed to download uv installer: {}", e)))?;

    if !status.status.success() {
        return Err(GitkaError::Config("Failed to download uv installer".to_string()));
    }

    // Run the installer script
    let installer_script = String::from_utf8_lossy(&status.stdout).to_string();
    let status = Command::new("sh")
        .arg("-c")
        .arg(&installer_script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
        .map_err(|e| GitkaError::Config(format!("Failed to run uv installer: {}", e)))?;

    if !status.success() {
        return Err(GitkaError::Config("Failed to install uv".to_string()));
    }

    // Add uv to PATH for this session
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let uv_bin = home.join(".local/bin");
    let current_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{}", uv_bin.display(), current_path));

    Ok(true)
}

/// Clean up uv if we installed it
fn cleanup_uv_if_installed(installed_by_us: bool) {
    if !installed_by_us {
        return;
    }

    // Uninstall uv silently
    let _ = Command::new("uv")
        .args(&["self", "uninstall", "-y"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Check if a process is running
fn is_process_running(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{}", pid)).exists()
    }

    #[cfg(not(target_os = "linux"))]
    {
        // Try to signal the process with signal 0
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
}

/// Kill a process
fn kill_process(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        Command::new("kill")
            .arg("-9")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[cfg(not(target_os = "linux"))]
    {
        unsafe { libc::kill(pid as i32, libc::SIGKILL) == 0 }
    }
}

/// Get the local IP address for LAN access
fn get_local_ip() -> Option<String> {
    // Try to get IP from route to 8.8.8.8
    let output = Command::new("ip")
        .args(&["route", "get", "8.8.8.8"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(src_pos) = line.find("src ") {
            let rest = &line[src_pos + 4..];
            if let Some(end) = rest.find(' ') {
                let ip = &rest[..end];
                if ip.parse::<std::net::Ipv4Addr>().is_ok() {
                    return Some(ip.to_string());
                }
            }
        }
    }

    None
}

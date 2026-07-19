use serde::{Deserialize, Serialize};
use std::process::Command;

// ── Types ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct RepoStatus {
    pub name: String,
    pub state: String,
    pub last_synced: String,
    pub archive_size: String,
    pub session: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncResult {
    pub success: bool,
    pub output: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitkaConfig {
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UsbDrive {
    pub path: String,
    pub label: String,
    pub size: String,
    pub mountpoint: String,
}

// ── Helpers ───────────────────────────────────────────────────────

fn gitka_cmd(args: &[&str]) -> Result<String, String> {
    let output = Command::new("gitka")
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run gitka: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout)
    } else {
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

// ── Tauri Commands ────────────────────────────────────────────────

#[tauri::command]
fn get_status(config_path: Option<String>) -> Result<Vec<RepoStatus>, String> {
    let args = if let Some(ref cfg) = config_path {
        vec!["--config", cfg.as_str(), "status"]
    } else {
        vec!["status"]
    };

    let output = gitka_cmd(&args)?;
    let mut repos = Vec::new();

    for line in output.lines() {
        // Parse status output: name state last_synced archive_size session
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 && !parts[0].starts_with('-') && parts[0] != "Name" && parts[0] != "Repository" {
            repos.push(RepoStatus {
                name: parts[0].to_string(),
                state: parts[1].to_string(),
                last_synced: parts.get(2).unwrap_or(&"").to_string(),
                archive_size: parts.get(3).unwrap_or(&"").to_string(),
                session: parts[4..].join(" "),
            });
        }
    }

    Ok(repos)
}

#[tauri::command]
fn sync_repos(config_path: Option<String>, repos: Option<Vec<String>>) -> Result<SyncResult, String> {
    let mut args: Vec<&str> = Vec::new();
    if let Some(ref cfg) = config_path {
        args.push("--config");
        args.push(cfg.as_str());
    }
    args.push("sync");

    if let Some(ref repo_list) = repos {
        args.push("--repos");
        for r in repo_list {
            args.push(r);
        }
    }

    let output = gitka_cmd(&args)?;
    Ok(SyncResult {
        success: true,
        output,
    })
}

#[tauri::command]
fn unlock_repo(config_path: Option<String>, repo: String) -> Result<String, String> {
    let mut args: Vec<&str> = Vec::new();
    if let Some(ref cfg) = config_path {
        args.push("--config");
        args.push(cfg.as_str());
    }
    args.push("unlock");
    args.push(&repo);

    gitka_cmd(&args)
}

#[tauri::command]
fn lock_repo(config_path: Option<String>, repo: String) -> Result<String, String> {
    let mut args: Vec<&str> = Vec::new();
    if let Some(ref cfg) = config_path {
        args.push("--config");
        args.push(cfg.as_str());
    }
    args.push("lock");
    args.push(&repo);

    gitka_cmd(&args)
}

#[tauri::command]
fn serve_repo(config_path: Option<String>, repo: String) -> Result<String, String> {
    let mut args: Vec<&str> = Vec::new();
    if let Some(ref cfg) = config_path {
        args.push("--config");
        args.push(cfg.as_str());
    }
    args.push("serve");
    args.push(&repo);

    gitka_cmd(&args)
}

#[tauri::command]
fn stop_serve(config_path: Option<String>, repo: String) -> Result<String, String> {
    let mut args: Vec<&str> = Vec::new();
    if let Some(ref cfg) = config_path {
        args.push("--config");
        args.push(cfg.as_str());
    }
    args.push("serve");
    args.push(&repo);
    args.push("--stop");

    gitka_cmd(&args)
}

#[tauri::command]
fn verify_archives(config_path: Option<String>, repos: Option<Vec<String>>) -> Result<String, String> {
    let mut args: Vec<&str> = Vec::new();
    if let Some(ref cfg) = config_path {
        args.push("--config");
        args.push(cfg.as_str());
    }
    args.push("verify");

    if let Some(ref repo_list) = repos {
        args.push("--repos");
        for r in repo_list {
            args.push(r);
        }
    }

    gitka_cmd(&args)
}

#[tauri::command]
fn repair_repo(config_path: Option<String>, repo: String) -> Result<String, String> {
    let mut args: Vec<&str> = Vec::new();
    if let Some(ref cfg) = config_path {
        args.push("--config");
        args.push(cfg.as_str());
    }
    args.push("repair");
    args.push(&repo);

    gitka_cmd(&args)
}

#[tauri::command]
fn get_config(config_path: Option<String>) -> Result<GitkaConfig, String> {
    let mut args: Vec<&str> = Vec::new();
    if let Some(ref cfg) = config_path {
        args.push("--config");
        args.push(cfg.as_str());
    }
    args.push("config");

    let content = gitka_cmd(&args)?;
    Ok(GitkaConfig { content })
}

#[tauri::command]
fn set_config(config_path: Option<String>, key: String, value: String) -> Result<String, String> {
    let kv = format!("{}={}", key, value);

    let mut owned: Vec<String> = Vec::new();
    owned.push("--config".into());
    if let Some(ref cfg) = config_path {
        owned.push(cfg.clone());
    }
    owned.push("config".into());
    owned.push("--set".into());
    owned.push(kv);

    let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
    gitka_cmd(&refs)
}

#[tauri::command]
fn scan_repos(config_path: Option<String>) -> Result<String, String> {
    let mut args: Vec<&str> = Vec::new();
    if let Some(ref cfg) = config_path {
        args.push("--config");
        args.push(cfg.as_str());
    }
    args.push("scan");

    gitka_cmd(&args)
}

#[tauri::command]
fn import_repo(config_path: Option<String>, path: String, name: Option<String>) -> Result<String, String> {
    let mut args: Vec<&str> = Vec::new();
    if let Some(ref cfg) = config_path {
        args.push("--config");
        args.push(cfg.as_str());
    }
    args.push("import");
    args.push(&path);

    if let Some(ref n) = name {
        args.push("--name");
        args.push(n);
    }

    gitka_cmd(&args)
}

#[tauri::command]
fn train_dict(config_path: Option<String>) -> Result<String, String> {
    let mut args: Vec<&str> = Vec::new();
    if let Some(ref cfg) = config_path {
        args.push("--config");
        args.push(cfg.as_str());
    }
    args.push("train-dict");

    gitka_cmd(&args)
}

#[tauri::command]
fn init_backup(
    source: String,
    target: String,
    username: Option<String>,
    token: Option<String>,
    gitflare_url: Option<String>,
    volume_size: Option<u64>,
    dedup: Option<bool>,
) -> Result<String, String> {
    let mut owned: Vec<String> = Vec::new();
    owned.push("init".into());
    owned.push("--source".into());
    owned.push(source);
    owned.push("--target".into());
    owned.push(target);

    if let Some(u) = username {
        owned.push("--username".into());
        owned.push(u);
    }
    if let Some(t) = token {
        owned.push("--token".into());
        owned.push(t);
    }
    if let Some(g) = gitflare_url {
        owned.push("--gitflare-url".into());
        owned.push(g);
    }
    if let Some(v) = volume_size {
        owned.push("--volume-size".into());
        owned.push(v.to_string());
    }
    if let Some(d) = dedup {
        owned.push("--dedup".into());
        owned.push(d.to_string());
    }

    let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
    gitka_cmd(&refs)
}

#[tauri::command]
fn wipe_drive(
    target: String,
    source: String,
    username: Option<String>,
    token: Option<String>,
    gitflare_url: Option<String>,
    filesystem: Option<String>,
    yes: bool,
) -> Result<String, String> {
    let mut owned: Vec<String> = Vec::new();
    owned.push("wipe".into());
    owned.push("--target".into());
    owned.push(target);
    owned.push("--source".into());
    owned.push(source);

    if let Some(u) = username {
        owned.push("--username".into());
        owned.push(u);
    }
    if let Some(t) = token {
        owned.push("--token".into());
        owned.push(t);
    }
    if let Some(g) = gitflare_url {
        owned.push("--gitflare-url".into());
        owned.push(g);
    }
    if let Some(f) = filesystem {
        owned.push("--filesystem".into());
        owned.push(f);
    }
    if yes {
        owned.push("--yes".into());
    }

    let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
    gitka_cmd(&refs)
}

#[tauri::command]
fn detect_usb_drives() -> Result<Vec<UsbDrive>, String> {
    let output = gitka_cmd(&["usb"])?;

    let mut drives = Vec::new();
    for line in output.lines() {
        // Parse usb output: path label size mountpoint
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && !parts[0].starts_with('-') && parts[0] != "Path" && parts[0] != "No" {
            drives.push(UsbDrive {
                path: parts[0].to_string(),
                label: parts.get(1).unwrap_or(&"").to_string(),
                size: parts.get(2).unwrap_or(&"unknown").to_string(),
                mountpoint: parts.get(3).unwrap_or(&"").to_string(),
            });
        }
    }

    Ok(drives)
}

// ── Main ──────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_status,
            sync_repos,
            unlock_repo,
            lock_repo,
            serve_repo,
            stop_serve,
            verify_archives,
            repair_repo,
            get_config,
            set_config,
            scan_repos,
            import_repo,
            train_dict,
            init_backup,
            wipe_drive,
            detect_usb_drives,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

use serde::{Deserialize, Serialize};
use std::process::Command;
use std::path::PathBuf;

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

#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubAuthResult {
    pub authenticated: bool,
    pub verified: bool,
    pub login: Option<String>,
    pub name: Option<String>,
    pub username: Option<String>,
    pub config_saved: bool,
    pub config_path: Option<String>,
    pub message: String,
}

// ── Helpers ───────────────────────────────────────────────────────

fn resolve_gitka_binary() -> Command {
    let current_exe = std::env::current_exe().ok();
    let candidates: Vec<PathBuf> = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| {
            vec![
                dir.join(if cfg!(target_os = "windows") { "gitka.exe" } else { "gitka" }),
                dir.join(if cfg!(target_os = "windows") { "gitka-gui.exe" } else { "gitka-gui" }),
            ]
        }))
        .unwrap_or_default();

    for candidate in candidates {
        if current_exe.as_ref().is_some_and(|exe| exe == &candidate) {
            continue;
        }
        if candidate.exists() {
            return Command::new(candidate);
        }
    }

    Command::new("gitka")
}

fn gitka_cmd(args: &[&str]) -> Result<String, String> {
    let output = resolve_gitka_binary()
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
        vec!["--config", cfg.as_str(), "status", "--json"]
    } else {
        vec!["status", "--json"]
    };

    let output = gitka_cmd(&args)?;
    serde_json::from_str(&output).map_err(|e| format!("Failed to parse status JSON: {}", e))
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
    if let Some(ref cfg) = config_path {
        owned.push("--config".into());
        owned.push(cfg.clone());
    }
    owned.push("config".into());
    owned.push("--set".into());
    owned.push(kv);

    let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
    gitka_cmd(&refs)
}

#[tauri::command]
fn auth_github(
    config_path: Option<String>,
    username: Option<String>,
    token: Option<String>,
    verify: bool,
    status: bool,
) -> Result<GitHubAuthResult, String> {
    let mut owned: Vec<String> = Vec::new();
    if let Some(ref cfg) = config_path {
        owned.push("--config".into());
        owned.push(cfg.clone());
    }
    owned.push("auth".into());
    if let Some(u) = username {
        owned.push("--username".into());
        owned.push(u);
    }
    if let Some(t) = token {
        owned.push("--token".into());
        owned.push(t);
    }
    if verify {
        owned.push("--verify".into());
    }
    if status {
        owned.push("--status".into());
    }
    owned.push("--json".into());

    let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
    let output = gitka_cmd(&refs)?;
    serde_json::from_str(&output).map_err(|e| format!("Failed to parse auth JSON: {}", e))
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
fn train_dict(config_path: Option<String>, source: Option<String>) -> Result<String, String> {
    let mut args: Vec<&str> = Vec::new();
    if let Some(ref cfg) = config_path {
        args.push("--config");
        args.push(cfg.as_str());
    }
    args.push("train-dict");

    if let Some(ref src) = source {
        args.push("--source");
        args.push(src.as_str());
    }

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
    let output = gitka_cmd(&["usb", "--json"])?;
    serde_json::from_str(&output).map_err(|e| format!("Failed to parse USB JSON: {}", e))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DriveStatus {
    pub initialized: bool,
    pub repos: Vec<RepoStatus>,
}

#[tauri::command]
fn check_drive(target: String) -> Result<DriveStatus, String> {
    let config_path = format!("{}/.gitka/gitka.toml", target);

    // Check if initialized by trying to get status
    let args = vec!["--config", &config_path, "status", "--json"];
    match gitka_cmd(&args) {
        Ok(output) => {
            let repos: Vec<RepoStatus> = serde_json::from_str(&output)
                .map_err(|e| format!("Failed to parse status JSON: {}", e))?;
            Ok(DriveStatus { initialized: true, repos })
        }
        Err(_) => Ok(DriveStatus { initialized: false, repos: Vec::new() }),
    }
}

#[tauri::command]
fn detect_repos_on_drive(path: String) -> Result<Vec<String>, String> {
    use std::fs;
    let mut repos = Vec::new();

    let entries = fs::read_dir(&path).map_err(|e| format!("Cannot read directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Read error: {}", e))?;
        let p = entry.path();
        if p.is_dir() && p.join(".git").exists() {
            if let Some(name) = p.file_name() {
                repos.push(name.to_string_lossy().to_string());
            }
        }
    }

    Ok(repos)
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
            auth_github,
            scan_repos,
            import_repo,
            train_dict,
            init_backup,
            wipe_drive,
            detect_usb_drives,
            check_drive,
            detect_repos_on_drive,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

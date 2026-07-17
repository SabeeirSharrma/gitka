mod archive;
mod cli;
mod compress;
mod config;
mod encryption;
mod error;
mod recovery;
mod repo;
mod serve;
mod source;
mod sync;
mod usb;

use clap::Parser;
use std::path::PathBuf;

use cli::{Cli, Commands};
use config::Config;
use error::{GitkaError, Result};

fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Execute command - init doesn't need config
    match &cli.command {
        Commands::Gui => {
            println!("GUI not yet implemented. Use CLI commands instead.");
            println!("Run `gitka --help` for available commands.");
        }
        Commands::Init { source, target, username, token, gitflare_url, interactive } => {
            cmd_init(source, target, username.as_deref(), token.as_deref(), gitflare_url.as_deref(), *interactive)?;
            return Ok(());
        }
        _ => {}
    }

    // Load config for other commands
    let (config, config_path) = load_config(&cli)?;

    match cli.command {
        Commands::Scan => {
            cmd_scan(&config)?;
        }
        Commands::Sync { repos } => {
            cmd_sync(&config, repos)?;
        }
        Commands::Status { repos } => {
            cmd_status(&config, repos)?;
        }
        Commands::Unlock { repo } => {
            cmd_unlock(&config, &repo)?;
        }
        Commands::Lock { repo } => {
            cmd_lock(&config, &repo)?;
        }
        Commands::Serve { repo, stop } => {
            cmd_serve(&config, &repo, stop)?;
        }
        Commands::Verify { repos, verbose } => {
            cmd_verify(&config, repos, verbose)?;
        }
        Commands::Repair { repo } => {
            cmd_repair(&config, &repo)?;
        }
        Commands::Config { set, get } => {
            let mut config = config;
            cmd_config(&mut config, set, get, &config_path)?;
        }
        _ => unreachable!(),
    }

    Ok(())
}

/// Load config from file or create default
fn load_config(cli: &Cli) -> Result<(Config, PathBuf)> {
    let config_path = if let Some(path) = &cli.config {
        path.clone()
    } else {
        // Look for config in current directory or home directory
        let current_dir = std::env::current_dir().map_err(|e| GitkaError::Config(e.to_string()))?;
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

        let candidates = vec![
            current_dir.join("gitka.toml"),
            current_dir.join(".gitka").join("gitka.toml"),
            home_dir.join(".gitka").join("gitka.toml"),
        ];

        candidates.into_iter()
            .find(|p| p.exists())
            .ok_or_else(|| GitkaError::Config(
                "No config file found. Run `gitka init` to create one.".to_string()
            ))?
    };

    let config = Config::load(&config_path)?;
    Ok((config, config_path))
}

/// Initialize a new Gitka backup
fn cmd_init(source: &str, target: &PathBuf, username: Option<&str>, token: Option<&str>, gitflare_url: Option<&str>, interactive: bool) -> Result<()> {
    if interactive {
        return cmd_init_interactive(target);
    }

    println!("Initializing Gitka backup...");
    println!("Source: {}", source);
    println!("Target: {}", target.display());

    // Create directory structure
    std::fs::create_dir_all(target.join("repos").join("archive"))?;
    std::fs::create_dir_all(target.join("extract"))?;
    std::fs::create_dir_all(target.join("tools").join("gitflare"))?;
    std::fs::create_dir_all(target.join("tools").join("recovery"))?;
    std::fs::create_dir_all(target.join("recovery-data"))?;
    std::fs::create_dir_all(target.join(".gitka").join("logs"))?;
    std::fs::create_dir_all(target.join(".gitka").join("repos"))?;

    // Create config with source settings
    let mut config = Config::default();
    config.target.path = target.clone();

    match source {
        "github" => {
            config.source.github_username = username.map(|s| s.to_string());
            config.source.auth_token = token.map(|s| s.to_string());

            if config.source.github_username.is_none() {
                println!("\n⚠ Warning: No GitHub username provided.");
                println!("  Edit {} to set github_username", target.join(".gitka").join("gitka.toml").display());
            }
        }
        "gitflare" => {
            config.source.gitflare_url = gitflare_url.map(|s| s.to_string());
            config.source.auth_token = token.map(|s| s.to_string());

            if config.source.gitflare_url.is_none() {
                println!("\n⚠ Warning: No GitFlare URL provided.");
                println!("  Edit {} to set gitflare_url", target.join(".gitka").join("gitka.toml").display());
            }
        }
        _ => {
            return Err(GitkaError::Config(format!("Unknown source type: {}", source)));
        }
    }

    let config_path = target.join(".gitka").join("gitka.toml");
    config.save(&config_path)?;

    println!("✓ Directory structure created");
    println!("✓ Config file created at {}", config_path.display());
    println!("\nNext steps:");
    println!("1. Edit {} to configure your sources", config_path.display());
    println!("2. Run `gitka scan` to discover repos");
    println!("3. Run `gitka sync` to clone and back up repos");

    Ok(())
}

/// Interactive setup wizard
fn cmd_init_interactive(target: &PathBuf) -> Result<()> {
    use std::io::{self, Write};

    println!("🚀 Gitka Setup Wizard");
    println!("=====================\n");

    // Create directory structure
    std::fs::create_dir_all(target.join("repos").join("archive"))?;
    std::fs::create_dir_all(target.join("extract"))?;
    std::fs::create_dir_all(target.join("tools").join("gitflare"))?;
    std::fs::create_dir_all(target.join("tools").join("recovery"))?;
    std::fs::create_dir_all(target.join("recovery-data"))?;
    std::fs::create_dir_all(target.join(".gitka").join("logs"))?;
    std::fs::create_dir_all(target.join(".gitka").join("repos"))?;

    let mut config = Config::default();
    config.target.path = target.clone();

    // Source type
    println!("📦 Source Configuration");
    println!("-----------------------");
    print!("Source type (github/gitflare) [github]: ");
    io::stdout().flush()?;
    let mut source = String::new();
    io::stdin().read_line(&mut source)?;
    let source = source.trim();
    let source = if source.is_empty() { "github" } else { source };

    match source {
        "github" => {
            print!("GitHub username: ");
            io::stdout().flush()?;
            let mut username = String::new();
            io::stdin().read_line(&mut username)?;
            let username = username.trim().to_string();
            if !username.is_empty() {
                config.source.github_username = Some(username);
            }

            print!("GitHub token (optional, press Enter to skip): ");
            io::stdout().flush()?;
            let mut token = String::new();
            io::stdin().read_line(&mut token)?;
            let token = token.trim().to_string();
            if !token.is_empty() {
                config.source.auth_token = Some(token);
            }
        }
        "gitflare" => {
            print!("GitFlare URL: ");
            io::stdout().flush()?;
            let mut url = String::new();
            io::stdin().read_line(&mut url)?;
            let url = url.trim().to_string();
            if !url.is_empty() {
                config.source.gitflare_url = Some(url);
            }

            print!("GitFlare token (optional, press Enter to skip): ");
            io::stdout().flush()?;
            let mut token = String::new();
            io::stdin().read_line(&mut token)?;
            let token = token.trim().to_string();
            if !token.is_empty() {
                config.source.auth_token = Some(token);
            }
        }
        _ => {
            println!("⚠ Unknown source type: {}. Using github.", source);
        }
    }

    // Compression settings
    println!("\n🗜️  Compression Settings");
    println!("-----------------------");
    print!("Compression tier (auto/low/medium/high) [auto]: ");
    io::stdout().flush()?;
    let mut tier = String::new();
    io::stdin().read_line(&mut tier)?;
    let tier = tier.trim();
    config.compression.tier = match tier {
        "low" => config::CompressionTier::Low,
        "medium" => config::CompressionTier::Medium,
        "high" => config::CompressionTier::High,
        _ => config::CompressionTier::Auto,
    };

    // Feature toggles
    println!("\n🔧 Feature Toggles");
    println!("-----------------------");
    print!("Enable recovery records? (y/n) [n]: ");
    io::stdout().flush()?;
    let mut recovery = String::new();
    io::stdin().read_line(&mut recovery)?;
    config.toggles.recovery_records = recovery.trim().to_lowercase() == "y";

    print!("Enable encryption? (y/n) [n]: ");
    io::stdout().flush()?;
    let mut encryption = String::new();
    io::stdin().read_line(&mut encryption)?;
    config.toggles.encryption = encryption.trim().to_lowercase() == "y";

    // Save config
    let config_path = target.join(".gitka").join("gitka.toml");
    config.save(&config_path)?;

    println!("\n✅ Setup complete!");
    println!("✓ Directory structure created at {}", target.display());
    println!("✓ Config file created at {}", config_path.display());
    println!("\nNext steps:");
    println!("1. Run `gitka scan` to discover repos");
    println!("2. Run `gitka sync` to clone and back up repos");

    Ok(())
}

/// Scan sources and target, show size report
fn cmd_scan(config: &Config) -> Result<()> {
    println!("Scanning sources and target...");

    // Detect drives
    let drives = usb::detect_drives(&config.target.mode)?;
    println!("\nDetected drives:");
    for drive in &drives {
        println!(
            "  {} - {} ({:.1} GB free of {:.1} GB) {}",
            drive.mount_point.display(),
            drive.fs_type,
            drive.free_space as f64 / 1_073_741_824.0,
            drive.total_space as f64 / 1_073_741_824.0,
            if drive.is_removable { "[removable]" } else { "" }
        );
    }

    // Discover repos from source
    println!("\nDiscovering repos from source...");
    match source::create_source(&config.source) {
        Ok(source_provider) => {
            match source_provider.list_repos() {
                Ok(remote_repos) => {
                    println!("Found {} repos:", remote_repos.len());
                    for repo in &remote_repos {
                        println!(
                            "  {} ({:.1} MB) {}",
                            repo.full_name,
                            repo.size as f64 / 1_048_576.0,
                            if repo.private { "[private]" } else { "" }
                        );
                    }

                    // Show which repos are configured locally
                    let repo_manager = repo::RepoManager::new(config.clone());
                    let local_repos = repo_manager.list_repos()?;
                    let local_names: Vec<&str> = local_repos.iter().map(|r| r.name.as_str()).collect();

                    println!("\nLocal status:");
                    for remote_repo in &remote_repos {
                        if local_names.contains(&remote_repo.name.as_str()) {
                            let local_repo = local_repos.iter().find(|r| r.name == remote_repo.name).unwrap();
                            println!(
                                "  {} - {:?} ({:.1} MB archive)",
                                remote_repo.name,
                                local_repo.state,
                                local_repo.archive_size as f64 / 1_048_576.0
                            );
                        } else {
                            println!("  {} - not cloned yet", remote_repo.name);
                        }
                    }
                }
                Err(e) => {
                    println!("⚠ Failed to list repos: {}", e);
                    println!("  Check your source configuration and authentication.");
                }
            }
        }
        Err(e) => {
            println!("⚠ No source configured: {}", e);
            println!("  Edit your config to set github_username or gitflare_url.");
        }
    }

    // Budget check
    let repo_manager = repo::RepoManager::new(config.clone());
    let total_archive = repo_manager.total_archive_size()?;
    let total_decompressed = repo_manager.total_decompressed_size()?;
    let free_space = drives.first().map(|d| d.free_space).unwrap_or(0);

    let budget = compress::BudgetCheck::new(free_space, total_decompressed);
    let tier = budget.determine_tier(&config.compression);

    println!("\nBudget:");
    println!("  Archive size: {:.1} MB", total_archive as f64 / 1_048_576.0);
    println!("  Decompressed size: {:.1} MB", total_decompressed as f64 / 1_048_576.0);
    println!("  Free space: {:.1} MB", free_space as f64 / 1_048_576.0);
    println!("  Recommended tier: {:?}", tier);

    if budget.is_over_budget() {
        println!("\n⚠ Warning: Repos may not fit even at maximum compression!");
        println!("  Consider reducing repo count or using a larger drive.");
    }

    Ok(())
}

/// Sync repos
fn cmd_sync(config: &Config, repos: Option<Vec<String>>) -> Result<()> {
    println!("Syncing repos...");

    // Get source provider
    let source_provider = source::create_source(&config.source)?;
    let auth = source_provider.auth_method();

    // Get list of remote repos
    let remote_repos = source_provider.list_repos()?;

    // Filter repos if specific ones requested
    let repos_to_sync: Vec<&source::RemoteRepo> = match &repos {
        Some(names) => remote_repos.iter()
            .filter(|r| names.contains(&r.name))
            .collect(),
        None => remote_repos.iter().collect(),
    };

    let repo_manager = repo::RepoManager::new(config.clone());

    for remote_repo in repos_to_sync {
        println!("\nSyncing {}...", remote_repo.full_name);

        // Check if repo exists locally
        let local_meta = repo_manager.load_meta(&remote_repo.name).ok();

        if let Some(meta) = local_meta {
            // Repo exists, fetch updates
            if meta.state == repo::RepoState::Archived {
                println!("  Repo is archived, skipping fetch. Use `gitka unlock {}` to extract first.", remote_repo.name);
                continue;
            }

            let extraction_path = meta.extraction_path
                .ok_or_else(|| GitkaError::Extraction(format!("No extraction path for {}", remote_repo.name)))?;

            println!("  Fetching updates...");
            source::fetch_repo(&extraction_path, &auth)?;

            // Sync the repo
            match sync::sync_repo(config, &remote_repo.name) {
                Ok(status) => {
                    match status {
                        sync::SyncStatus::Ahead(n) => println!("  Pushed {} commits", n),
                        sync::SyncStatus::Behind(n) => println!("  Pulled {} commits", n),
                        sync::SyncStatus::InSync => println!("  Up to date"),
                        sync::SyncStatus::Diverged { ahead, behind } => {
                            println!("  Diverged ({} ahead, {} behind)", ahead, behind);
                        }
                        sync::SyncStatus::Conflict(msg) => {
                            println!("  CONFLICT: {}", msg);
                        }
                    }
                }
                Err(e) => {
                    println!("  ⚠ Sync failed: {}", e);
                }
            }
        } else {
            // Repo doesn't exist, clone it
            println!("  Cloning new repo...");

            let archive_dir = config.archive_dir();
            std::fs::create_dir_all(&archive_dir)?;

            // Determine shallow based on config
            let repo_config = config.get_repo(&remote_repo.name);
            let shallow = match repo_config {
                Ok(r) => !r.full_history,
                Err(_) => true, // Default to shallow
            };

            match source::clone_repo(remote_repo, &archive_dir, &auth, shallow) {
                Ok(repo_path) => {
                    let repo_size = source::repo_size(&repo_path)?;

                    // Create repo metadata
                    let meta = repo::RepoMeta {
                        name: remote_repo.name.clone(),
                        state: repo::RepoState::Archived,
                        archive_path: PathBuf::from(format!("{}.gitka.zst", remote_repo.name)),
                        archive_hash: None,
                        archive_size: 0, // Will be set after compression
                        decompressed_size: Some(repo_size),
                        last_synced: None,
                        last_verified: None,
                        extraction_path: None,
                    };

                    repo_manager.save_meta(&meta)?;

                    // Compress the repo
                    println!("  Compressing...");
                    let archive_path = archive_dir.join(format!("{}.gitka.zst", remote_repo.name));
                    match compress::compress_directory(&repo_path, &archive_path, &config.compression) {
                        Ok(archive_size) => {
                            // Calculate hash
                            let hash = compress::calculate_hash(&archive_path)?;

                            // Update metadata with archive info
                            let mut meta = repo_manager.load_meta(&remote_repo.name)?;
                            meta.archive_size = archive_size;
                            meta.archive_hash = Some(hash);
                            repo_manager.save_meta(&meta)?;

                            println!("  ✓ Cloned and compressed ({:.1} MB archive, {:.1} MB source)",
                                archive_size as f64 / 1_048_576.0,
                                repo_size as f64 / 1_048_576.0);

                            // Create recovery records if enabled
                            if config.toggles.recovery_records && recovery::is_par2_available() {
                                println!("  Creating recovery records...");
                                let recovery_dir = config.recovery_dir().join(&remote_repo.name);
                                match recovery::create_recovery(&archive_path, &recovery_dir, 25) {
                                    Ok(info) => {
                                        println!("  ✓ Recovery records created ({:.1} MB, {} blocks)",
                                            info.recovery_size as f64 / 1_048_576.0,
                                            info.block_count);
                                    }
                                    Err(e) => {
                                        println!("  ⚠ Recovery record creation failed: {}", e);
                                    }
                                }
                            }

                            // Clean up the extracted repo
                            std::fs::remove_dir_all(&repo_path)?;
                        }
                        Err(e) => {
                            println!("  ⚠ Compression failed: {}", e);
                            println!("  Repo remains at: {}", repo_path.display());
                        }
                    }
                }
                Err(e) => {
                    println!("  ⚠ Clone failed: {}", e);
                }
            }
        }
    }

    println!("\n✓ Sync complete");
    Ok(())
}

/// Show status
fn cmd_status(config: &Config, repos: Option<Vec<String>>) -> Result<()> {
    let repo_manager = repo::RepoManager::new(config.clone());
    let all_repos = repo_manager.list_repos()?;

    let repos_to_show: Vec<&repo::RepoMeta> = match repos {
        Some(names) => all_repos.iter()
            .filter(|r| names.contains(&r.name))
            .collect(),
        None => all_repos.iter().collect(),
    };

    println!("Repository Status:");
    println!("{:<30} {:<15} {:<15} {:<20}", "Name", "State", "Last Synced", "Archive Size");
    println!("{}", "-".repeat(80));

    for repo in repos_to_show {
        println!(
            "{:<30} {:<15} {:<15} {:<20}",
            repo.name,
            format!("{:?}", repo.state),
            repo.last_synced.as_deref().unwrap_or("never"),
            format!("{:.1} MB", repo.archive_size as f64 / 1_048_576.0)
        );
    }

    Ok(())
}

/// Unlock a repo for offline access
fn cmd_unlock(config: &Config, repo_name: &str) -> Result<()> {
    println!("Unlocking {} for offline access...", repo_name);

    let repo_manager = repo::RepoManager::new(config.clone());
    let mut meta = repo_manager.load_meta(repo_name)?;

    // Check if already extracted
    if meta.state != repo::RepoState::Archived {
        return Err(GitkaError::AlreadyExtracted(repo_name.to_string()));
    }

    // Check workspace eligibility — if repo is in config, check the flag; otherwise default to eligible
    if let Ok(repo_config) = config.get_repo(repo_name) {
        if !repo_config.workspace_eligible {
            return Err(GitkaError::NotWorkspaceEligible(repo_name.to_string()));
        }
    }

    // Budget check
    let free_space = usb::get_drive_info(&config.target.path)?.free_space;
    let decompressed_size = meta.decompressed_size.unwrap_or(0);
    let budget = compress::BudgetCheck::new(free_space, decompressed_size);

    if budget.is_over_budget() {
        println!("⚠ Warning: This repo may not fit for extraction!");
        println!("  Consider using extraction target 'host' instead.");
    }

    // Extract the repo
    let extraction_path = meta.extraction_target(config);
    std::fs::create_dir_all(&extraction_path)?;

    // Decompress archive
    let archive_path = meta.archive_full_path(config);
    compress::decompress_directory(&archive_path, &extraction_path)?;

    // Update metadata
    meta.state = repo::RepoState::ExtractedLocal;
    meta.extraction_path = Some(extraction_path.clone());
    repo_manager.save_meta(&meta)?;

    println!("✓ Repo extracted to {}", extraction_path.display());
    println!("  You can now make commits offline.");
    println!("  Run `gitka lock {}` when done.", repo_name);

    Ok(())
}

/// Lock a repo (recompress and clear extraction)
fn cmd_lock(config: &Config, repo_name: &str) -> Result<()> {
    println!("Locking {}...", repo_name);

    let repo_manager = repo::RepoManager::new(config.clone());
    let mut meta = repo_manager.load_meta(repo_name)?;

    // Check if extracted
    if meta.state == repo::RepoState::Archived {
        return Err(GitkaError::NotExtracted(repo_name.to_string()));
    }

    let extraction_path = meta.extraction_path
        .ok_or_else(|| GitkaError::Extraction(format!("No extraction path for {}", repo_name)))?;

    // Recompress
    let archive_path = config.archive_dir().join(format!("{}.gitka.zst", repo_name));
    std::fs::create_dir_all(config.archive_dir())?;

    let new_size = compress::compress_directory(&extraction_path, &archive_path, &config.compression)?;
    let new_hash = compress::calculate_hash(&archive_path)?;

    // Verify before deleting old archive
    if config.toggles.verify_after_sync {
        compress::verify_archive(&archive_path)?;
    }

    // Clean up extraction
    if config.toggles.clear_after_lock {
        std::fs::remove_dir_all(&extraction_path)?;
    }

    // Update metadata
    meta.state = repo::RepoState::Archived;
    meta.archive_path = PathBuf::from(format!("{}.gitka.zst", repo_name));
    meta.archive_hash = Some(new_hash);
    meta.archive_size = new_size;
    meta.extraction_path = None;
    repo_manager.save_meta(&meta)?;

    println!("✓ Repo locked and recompressed");
    println!("  Archive: {} ({:.1} MB)", archive_path.display(), new_size as f64 / 1_048_576.0);

    Ok(())
}

/// Serve a repo via GitFlare
fn cmd_serve(config: &Config, repo_name: &str, stop: bool) -> Result<()> {
    if stop {
        println!("Stopping GitFlare server for {}...", repo_name);
        match serve::stop(config, repo_name)? {
            true => println!("✓ Server stopped"),
            false => println!("  No server running for {}", repo_name),
        }
    } else {
        println!("Starting GitFlare server for {}...", repo_name);
        let info = serve::start(config, repo_name)?;
        println!("✓ Server started on port {}", info.port);
        println!();
        println!("  Clone URL (local):   {}", info.clone_url());
        println!("  Clone URL (LAN):     {}/{}.git", info.lan_url(), repo_name);
        println!();
        println!("  Other machines on your LAN can clone with:");
        println!("    git clone {}/{}.git", info.lan_url(), repo_name);
        println!();
        println!("  Run `gitka serve {} --stop` to stop.", repo_name);
    }

    Ok(())
}

/// Verify archive integrity
fn cmd_verify(config: &Config, repos: Option<Vec<String>>, verbose: bool) -> Result<()> {
    let repo_manager = repo::RepoManager::new(config.clone());
    let all_repos = repo_manager.list_repos()?;

    let repos_to_verify: Vec<&repo::RepoMeta> = match repos {
        Some(names) => all_repos.iter()
            .filter(|r| names.contains(&r.name))
            .collect(),
        None => all_repos.iter().collect(),
    };

    if repos_to_verify.is_empty() {
        println!("No repos to verify.");
        return Ok(());
    }

    println!("Verifying {} archive(s)...\n", repos_to_verify.len());

    let mut all_ok = true;

    for repo in &repos_to_verify {
        let archive_path = repo.archive_full_path(config);
        let mut checks = Vec::new();

        // 1. Check archive exists
        if !archive_path.exists() {
            println!("✗ {} - archive not found: {}", repo.name, archive_path.display());
            all_ok = false;
            continue;
        }

        // 2. Check file size
        let file_size = std::fs::metadata(&archive_path)
            .map(|m| m.len())
            .unwrap_or(0);
        if file_size == 0 {
            println!("✗ {} - archive is empty", repo.name);
            all_ok = false;
            continue;
        }
        checks.push(format!("size: {:.1} MB", file_size as f64 / 1_048_576.0));

        // 3. Check archive header
        match archive::Archive::open(&archive_path) {
            Ok(_archive) => checks.push("header: OK".to_string()),
            Err(e) => {
                println!("✗ {} - header check failed: {}", repo.name, e);
                all_ok = false;
                continue;
            }
        }

        // 4. Verify compression integrity
        match compress::verify_archive(&archive_path) {
            Ok(()) => checks.push("decompression: OK".to_string()),
            Err(e) => {
                println!("✗ {} - decompression failed: {}", repo.name, e);
                all_ok = false;
                continue;
            }
        }

        // 5. Verify hash if stored
        if let Some(stored_hash) = &repo.archive_hash {
            match compress::calculate_hash(&archive_path) {
                Ok(computed_hash) => {
                    if &computed_hash == stored_hash {
                        checks.push("hash: OK".to_string());
                    } else {
                        println!("✗ {} - hash mismatch!", repo.name);
                        println!("  expected: {}", stored_hash);
                        println!("  computed: {}", computed_hash);
                        all_ok = false;
                        continue;
                    }
                }
                Err(e) => {
                    println!("✗ {} - hash calculation failed: {}", repo.name, e);
                    all_ok = false;
                    continue;
                }
            }
        } else {
            checks.push("hash: not stored".to_string());
        }

        // 6. Check recovery records availability
        let recovery_path = config.recovery_dir().join(format!("{}.par2", repo.name));
        if recovery_path.exists() {
            checks.push("recovery: available".to_string());
        } else {
            checks.push("recovery: none".to_string());
        }

        // 7. Check permissions (not writable by others)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&archive_path) {
                let mode = meta.permissions().mode();
                let others_writable = (mode & 0o002) != 0;
                if others_writable {
                    checks.push("perms: world-writable (warning)".to_string());
                } else {
                    checks.push("perms: OK".to_string());
                }
            }
        }

        println!("✓ {} - {}", repo.name, checks.join(", "));
        if verbose {
            println!("  archive: {}", archive_path.display());
            println!("  hash:    {}", repo.archive_hash.as_deref().unwrap_or("n/a"));
            println!();
        }
    }

    if all_ok {
        println!("\nAll archives verified successfully.");
    } else {
        println!("\nSome archives failed verification.");
    }

    Ok(())
}

/// Repair a corrupted repo
fn cmd_repair(config: &Config, repo_name: &str) -> Result<()> {
    println!("Repairing {}...\n", repo_name);

    // Check if par2 is available
    if !recovery::is_par2_available() {
        println!("✗ par2 not found. Install it:");
        println!("  sudo apt install par2");
        println!();
        println!("Recovery records require par2 to create and repair.");
        return Ok(());
    }

    // Get repo metadata
    let repo_manager = repo::RepoManager::new(config.clone());
    let meta = repo_manager.load_meta(repo_name)?;

    let archive_path = meta.archive_full_path(config);
    let recovery_dir = config.recovery_dir().join(repo_name);

    // Check if recovery records exist
    if !recovery_dir.exists() {
        println!("✗ No recovery records found for {}", repo_name);
        println!("  Recovery dir: {}", recovery_dir.display());
        println!();
        println!("To create recovery records, run:");
        println!("  gitka sync --repos {} (with toggles.recovery_records = true)", repo_name);
        return Ok(());
    }

    // Verify recovery records first
    println!("Verifying recovery records...");
    match recovery::verify_recovery(&recovery_dir) {
        Ok(true) => println!("  ✓ Recovery records are valid"),
        Ok(false) => {
            println!("  ✗ Recovery records are corrupted");
            println!("  Cannot repair without valid recovery records");
            return Ok(());
        }
        Err(e) => {
            println!("  ✗ Failed to verify recovery records: {}", e);
            return Ok(());
        }
    }

    // Check if the archive is corrupted
    println!("\nChecking archive integrity...");
    let archive_ok = match compress::verify_archive(&archive_path) {
        Ok(()) => {
            println!("  ✓ Archive is intact");
            true
        }
        Err(e) => {
            println!("  ✗ Archive is corrupted: {}", e);
            false
        }
    };

    if archive_ok {
        println!("\nArchive is not corrupted. No repair needed.");
        return Ok(());
    }

    // Attempt repair
    println!("\nAttempting repair...");
    match recovery::repair_file(&archive_path, &recovery_dir) {
        Ok(()) => {
            println!("  ✓ Repair successful");

            // Verify the repaired file
            println!("\nVerifying repaired archive...");
            match compress::verify_archive(&archive_path) {
                Ok(()) => println!("  ✓ Repaired archive is valid"),
                Err(e) => {
                    println!("  ✗ Repaired archive still has issues: {}", e);
                    println!("  The recovery records may not have enough redundancy.");
                }
            }
        }
        Err(e) => {
            println!("  ✗ Repair failed: {}", e);
            println!();
            println!("  The recovery records may not have enough redundancy.");
            println!("  Consider re-syncing the repo from source.");
        }
    }

    Ok(())
}

/// View/edit config
fn cmd_config(config: &mut Config, set: Option<String>, get: Option<String>, config_path: &std::path::Path) -> Result<()> {
    if let Some(key) = get {
        // Get a config value
        match key.as_str() {
            "source.github_username" => println!("{}", config.source.github_username.as_deref().unwrap_or("not set")),
            "source.gitflare_url" => println!("{}", config.source.gitflare_url.as_deref().unwrap_or("not set")),
            "source.auth_token" => {
                if config.source.auth_token.is_some() {
                    println!("*** (set)");
                } else {
                    println!("not set");
                }
            }
            "target.path" => println!("{}", config.target.path.display()),
            "target.mode" => println!("{:?}", config.target.mode),
            "compression.backend" => println!("{:?}", config.compression.backend),
            "compression.tier" => println!("{:?}", config.compression.tier),
            "compression.dictionary_size_mb" => println!("{}", config.compression.dictionary_size_mb),
            "compression.dedup" => println!("{}", config.compression.dedup),
            "extraction.target" => println!("{:?}", config.extraction.target),
            "toggles.clear_after_lock" => println!("{}", config.toggles.clear_after_lock),
            "toggles.verify_after_sync" => println!("{}", config.toggles.verify_after_sync),
            "toggles.encryption" => println!("{}", config.toggles.encryption),
            "toggles.recovery_records" => println!("{}", config.toggles.recovery_records),
            "integrations.gitflare.port" => {
                println!("{}", config.integrations.gitflare.as_ref().map(|g| g.port).unwrap_or(8080));
            }
            "integrations.gitflare.bind_address" => {
                println!("{}", config.integrations.gitflare.as_ref().map(|g| g.bind_address.as_str()).unwrap_or("0.0.0.0"));
            }
            _ => {
                println!("Unknown config key: {}", key);
                println!("\nAvailable keys:");
                println!("  source.github_username");
                println!("  source.gitflare_url");
                println!("  source.auth_token");
                println!("  target.path");
                println!("  target.mode              (removable | local)");
                println!("  compression.backend      (zstd)");
                println!("  compression.tier         (auto | low | medium | high)");
                println!("  compression.dictionary_size_mb");
                println!("  compression.dedup        (true | false)");
                println!("  extraction.target        (usb | host)");
                println!("  toggles.clear_after_lock (true | false)");
                println!("  toggles.verify_after_sync (true | false)");
                println!("  toggles.encryption       (true | false)");
                println!("  toggles.recovery_records (true | false)");
                println!("  integrations.gitflare.port");
                println!("  integrations.gitflare.bind_address");
            }
        }
    } else if let Some(kv) = set {
        // Set a config value: key=value format
        let parts: Vec<&str> = kv.splitn(2, '=').collect();
        if parts.len() != 2 {
            println!("Invalid format. Use: key=value");
            return Ok(());
        }
        let key = parts[0].trim();
        let value = parts[1].trim();

        match key {
            "source.github_username" => config.source.github_username = Some(value.to_string()),
            "source.gitflare_url" => config.source.gitflare_url = Some(value.to_string()),
            "source.auth_token" => config.source.auth_token = Some(value.to_string()),
            "target.path" => config.target.path = std::path::PathBuf::from(value),
            "target.mode" => {
                match value {
                    "removable" => config.target.mode = config::TargetMode::Removable,
                    "local" => config.target.mode = config::TargetMode::Local,
                    _ => { println!("Invalid mode. Use: removable | local"); return Ok(()); }
                }
            }
            "compression.backend" => {
                match value {
                    "zstd" => config.compression.backend = config::CompressionBackend::Zstd,
                    _ => { println!("Invalid backend. Use: zstd"); return Ok(()); }
                }
            }
            "compression.tier" => {
                match value {
                    "auto" => config.compression.tier = config::CompressionTier::Auto,
                    "low" => config.compression.tier = config::CompressionTier::Low,
                    "medium" => config.compression.tier = config::CompressionTier::Medium,
                    "high" => config.compression.tier = config::CompressionTier::High,
                    _ => { println!("Invalid tier. Use: auto | low | medium | high"); return Ok(()); }
                }
            }
            "compression.dictionary_size_mb" => {
                match value.parse::<u32>() {
                    Ok(n) => config.compression.dictionary_size_mb = n,
                    Err(_) => { println!("Invalid number: {}", value); return Ok(()); }
                }
            }
            "compression.dedup" => {
                match value {
                    "true" => config.compression.dedup = true,
                    "false" => config.compression.dedup = false,
                    _ => { println!("Invalid value. Use: true | false"); return Ok(()); }
                }
            }
            "extraction.target" => {
                match value {
                    "usb" => config.extraction.target = config::ExtractionTarget::Usb,
                    "host" => config.extraction.target = config::ExtractionTarget::Host,
                    _ => { println!("Invalid target. Use: usb | host"); return Ok(()); }
                }
            }
            "toggles.clear_after_lock" => {
                match value {
                    "true" => config.toggles.clear_after_lock = true,
                    "false" => config.toggles.clear_after_lock = false,
                    _ => { println!("Invalid value. Use: true | false"); return Ok(()); }
                }
            }
            "toggles.verify_after_sync" => {
                match value {
                    "true" => config.toggles.verify_after_sync = true,
                    "false" => config.toggles.verify_after_sync = false,
                    _ => { println!("Invalid value. Use: true | false"); return Ok(()); }
                }
            }
            "toggles.encryption" => {
                match value {
                    "true" => config.toggles.encryption = true,
                    "false" => config.toggles.encryption = false,
                    _ => { println!("Invalid value. Use: true | false"); return Ok(()); }
                }
            }
            "toggles.recovery_records" => {
                match value {
                    "true" => config.toggles.recovery_records = true,
                    "false" => config.toggles.recovery_records = false,
                    _ => { println!("Invalid value. Use: true | false"); return Ok(()); }
                }
            }
            "integrations.gitflare.port" => {
                match value.parse::<u16>() {
                    Ok(n) => {
                        if config.integrations.gitflare.is_none() {
                            config.integrations.gitflare = Some(config::GitFlareConfig {
                                port: n,
                                bind_address: "0.0.0.0".to_string(),
                            });
                        } else {
                            config.integrations.gitflare.as_mut().unwrap().port = n;
                        }
                    }
                    Err(_) => { println!("Invalid port number: {}", value); return Ok(()); }
                }
            }
            "integrations.gitflare.bind_address" => {
                if config.integrations.gitflare.is_none() {
                    config.integrations.gitflare = Some(config::GitFlareConfig {
                        port: 8080,
                        bind_address: value.to_string(),
                    });
                } else {
                    config.integrations.gitflare.as_mut().unwrap().bind_address = value.to_string();
                }
            }
            _ => {
                println!("Unknown config key: {}", key);
                println!("Run `gitka config --get unknown_key` to see available keys.");
                return Ok(());
            }
        }

        // Save config
        config.save(config_path)?;
        println!("✓ {} = {}", key, value);
    } else {
        // Show full config
        println!("Current configuration:");
        println!("{}", toml::to_string_pretty(config).unwrap());
        println!("\nConfig file: {}", config_path.display());
        println!("\nUse `gitka config --set key=value` to edit.");
        println!("Use `gitka config --get key` to query.");
    }

    Ok(())
}

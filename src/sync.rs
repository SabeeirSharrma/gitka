use crate::config::Config;
use crate::error::{GitkaError, Result};
use crate::repo::{RepoMeta, RepoState};

/// Sync status for a repository
#[derive(Debug, Clone)]
pub enum SyncStatus {
    /// Local is ahead of origin
    Ahead(u32),
    /// Origin is ahead of local
    Behind(u32),
    /// Local and origin have diverged
    Diverged {
        ahead: u32,
        behind: u32,
    },
    /// Local and origin are in sync
    InSync,
    /// Conflict requires manual resolution
    Conflict(String),
}

/// Sync a repository (fetch/compare/push/pull/merge loop)
pub fn sync_repo(config: &Config, repo_name: &str) -> Result<SyncStatus> {
    let repo_manager = crate::repo::RepoManager::new(config.clone());
    let meta = repo_manager.load_meta(repo_name)?;

    // Check if repo is extracted
    if meta.state == RepoState::Archived {
        return Err(GitkaError::NotExtracted(repo_name.to_string()));
    }

    let extraction_path = meta.extraction_path
        .ok_or_else(|| GitkaError::Extraction(format!("No extraction path for {}", repo_name)))?;

    // Open the git repository
    let repo = git2::Repository::open(&extraction_path)
        .map_err(|e| GitkaError::Git(e))?;

    // Fetch from origin
    let mut remote = repo.find_remote("origin")
        .map_err(|e| GitkaError::Git(e))?;

    remote.fetch(&[] as &[&str], None, None)
        .map_err(|e| GitkaError::Git(e))?;

    // Get local and remote HEADs
    let local_head = repo.head()
        .map_err(|e| GitkaError::Git(e))?;

    let remote_branch = repo.find_branch("origin/HEAD", git2::BranchType::Remote)
        .map_err(|e| GitkaError::Git(e))?;
    let remote_head = remote_branch.get().clone();

    let local_oid = local_head.target()
        .ok_or_else(|| GitkaError::Git(git2::Error::from_str("No local HEAD")))?;

    let remote_oid = remote_head.target()
        .ok_or_else(|| GitkaError::Git(git2::Error::from_str("No remote HEAD")))?;

    // Compare commits
    if local_oid == remote_oid {
        return Ok(SyncStatus::InSync);
    }

    // Count ahead/behind
    let (ahead, behind) = repo.graph_ahead_behind(local_oid, remote_oid)
        .map_err(|e| GitkaError::Git(e))?;

    if ahead > 0 && behind > 0 {
        // Diverged - try auto-merge
        return attempt_merge(&repo, repo_name, local_oid, remote_oid);
    } else if ahead > 0 {
        // Local ahead - push
        remote.push(&["refs/heads/main:refs/heads/main"], None)
            .map_err(|e| GitkaError::Git(e))?;
        return Ok(SyncStatus::Ahead(ahead as u32));
    } else if behind > 0 {
        // Remote ahead - pull (fast-forward)
        let mut branch = repo.find_branch("main", git2::BranchType::Local)
            .map_err(|e| GitkaError::Git(e))?;
        branch.set_upstream(Some("origin/main"))
            .map_err(|e| GitkaError::Git(e))?;

        // Checkout the remote commit to fast-forward
        let remote_commit = repo.find_commit(remote_oid)
            .map_err(|e| GitkaError::Git(e))?;
        repo.checkout_tree(remote_commit.as_object(), None)
            .map_err(|e| GitkaError::Git(e))?;

        // Update the branch reference
        branch.get_mut().set_target(remote_oid, "fast-forward")
            .map_err(|e| GitkaError::Git(e))?;

        return Ok(SyncStatus::Behind(behind as u32));
    }

    // Shouldn't reach here, but handle gracefully
    Ok(SyncStatus::InSync)
}

/// Attempt to merge diverged branches
fn attempt_merge(
    repo: &git2::Repository,
    repo_name: &str,
    local_oid: git2::Oid,
    remote_oid: git2::Oid,
) -> Result<SyncStatus> {
    // Get the merge base
    let merge_base = repo.merge_base(local_oid, remote_oid)
        .map_err(|_| GitkaError::SyncConflict(format!("{}: no merge base found", repo_name)))?;

    // Get the commits
    let local_commit = repo.find_commit(local_oid)
        .map_err(|e| GitkaError::Git(e))?;
    let remote_commit = repo.find_commit(remote_oid)
        .map_err(|e| GitkaError::Git(e))?;
    let base_commit = repo.find_commit(merge_base)
        .map_err(|e| GitkaError::Git(e))?;

    // Try to merge (takes 2 commits and an optional ancestor)
    let mut index = repo.merge_commits(&base_commit, &local_commit, None)
        .map_err(|e| GitkaError::Git(e))?;

    // Check for conflicts
    if index.has_conflicts() {
        let conflicts: Vec<String> = index.conflicts()
            .map_err(|e| GitkaError::Git(e))?
            .filter_map(|c| c.ok())
            .map(|c| {
                let ancestor = c.ancestor.map(|a| String::from_utf8_lossy(&a.path).to_string()).unwrap_or_default();
                let ours = c.our.map(|o| String::from_utf8_lossy(&o.path).to_string()).unwrap_or_default();
                let theirs = c.their.map(|t| String::from_utf8_lossy(&t.path).to_string()).unwrap_or_default();
                format!("{} (ours: {}, theirs: {})", ancestor, ours, theirs)
            })
            .collect();

        return Err(GitkaError::SyncConflict(format!(
            "{}: {} conflicts",
            repo_name,
            conflicts.join(", ")
        )));
    }

    // If no conflicts, we could auto-merge
    // For now, flag as needing manual resolution
    let (ahead, behind) = repo.graph_ahead_behind(local_oid, remote_oid)
        .map_err(|e| GitkaError::Git(e))?;

    Ok(SyncStatus::Diverged {
        ahead: ahead as u32,
        behind: behind as u32,
    })
}

/// Sync all eligible repos
pub fn sync_all(config: &Config, repos: Option<Vec<String>>) -> Result<Vec<(String, SyncStatus)>> {
    let repo_manager = crate::repo::RepoManager::new(config.clone());
    let all_repos = repo_manager.list_repos()?;

    let repos_to_sync: Vec<&RepoMeta> = match repos {
        Some(names) => all_repos.iter()
            .filter(|r| names.contains(&r.name))
            .collect(),
        None => all_repos.iter()
            .filter(|r| r.state != RepoState::Archived)
            .collect(),
    };

    let mut results = Vec::new();

    for repo_meta in repos_to_sync {
        match sync_repo(config, &repo_meta.name) {
            Ok(status) => results.push((repo_meta.name.clone(), status)),
            Err(e) => {
                tracing::error!("Failed to sync {}: {}", repo_meta.name, e);
                // Continue with other repos
            }
        }
    }

    Ok(results)
}

/// Check sync status without performing any operations
pub fn check_sync_status(config: &Config, repo_name: &str) -> Result<SyncStatus> {
    let repo_manager = crate::repo::RepoManager::new(config.clone());
    let meta = repo_manager.load_meta(repo_name)?;

    if meta.state == RepoState::Archived {
        return Err(GitkaError::NotExtracted(repo_name.to_string()));
    }

    let extraction_path = meta.extraction_path
        .ok_or_else(|| GitkaError::Extraction(format!("No extraction path for {}", repo_name)))?;

    let repo = git2::Repository::open(&extraction_path)
        .map_err(|e| GitkaError::Git(e))?;

    // Get local and remote HEADs
    let local_head = repo.head()
        .map_err(|e| GitkaError::Git(e))?;

    let remote_branch = match repo.find_branch("origin/HEAD", git2::BranchType::Remote) {
        Ok(branch) => branch,
        Err(_) => {
            // If origin/HEAD doesn't exist, try to find any remote branch
            let mut remote = repo.find_remote("origin")
                .map_err(|e| GitkaError::Git(e))?;
            remote.fetch(&[] as &[&str], None, None)
                .map_err(|e| GitkaError::Git(e))?;

            repo.find_branch("origin/main", git2::BranchType::Remote)
                .map_err(|e| GitkaError::Git(e))?
        }
    };

    let remote_head = remote_branch.get().clone();

    let local_oid = local_head.target()
        .ok_or_else(|| GitkaError::Git(git2::Error::from_str("No local HEAD")))?;

    let remote_oid = remote_head.target()
        .ok_or_else(|| GitkaError::Git(git2::Error::from_str("No remote HEAD")))?;

    if local_oid == remote_oid {
        return Ok(SyncStatus::InSync);
    }

    let (ahead, behind) = repo.graph_ahead_behind(local_oid, remote_oid)
        .map_err(|e| GitkaError::Git(e))?;

    if ahead > 0 && behind > 0 {
        Ok(SyncStatus::Diverged {
            ahead: ahead as u32,
            behind: behind as u32,
        })
    } else if ahead > 0 {
        Ok(SyncStatus::Ahead(ahead as u32))
    } else if behind > 0 {
        Ok(SyncStatus::Behind(behind as u32))
    } else {
        Ok(SyncStatus::InSync)
    }
}

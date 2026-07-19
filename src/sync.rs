#![allow(dead_code)]

use crate::config::Config;
use crate::error::{GitkaError, Result};
use crate::repo::{RepoMeta, RepoState};
use std::borrow::Cow;

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

/// Detect the default branch name from origin/HEAD or fall back to "main".
fn default_branch(repo: &git2::Repository) -> Cow<'static, str> {
    // Try origin/HEAD symbolic ref first
    if let Ok(remote_head) = repo.find_reference("refs/remotes/origin/HEAD") {
        if let Some(target) = remote_head.symbolic_target() {
            if let Some(branch) = target.strip_prefix("refs/remotes/origin/") {
                return Cow::Owned(branch.to_string());
            }
        }
    }
    // Fall back to checking common branch names
    for candidate in &["main", "master", "trunk"] {
        if repo.find_reference(&format!("refs/remotes/origin/{}", candidate)).is_ok() {
            return Cow::Owned(candidate.to_string());
        }
    }
    Cow::Borrowed("main")
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

    let branch = default_branch(&repo);

    // Fetch from origin
    let mut remote = repo.find_remote("origin")
        .map_err(|e| GitkaError::Git(e))?;

    remote.fetch(&[] as &[&str], None, None)
        .map_err(|e| GitkaError::Git(e))?;

    // Get local and remote HEADs
    let local_head = repo.head()
        .map_err(|e| GitkaError::Git(e))?;

    let remote_ref = format!("refs/remotes/origin/{}", branch);
    let remote_branch = repo.find_branch(&remote_ref, git2::BranchType::Remote)
        .or_else(|_| repo.find_branch("origin/HEAD", git2::BranchType::Remote))
        .map_err(|e| GitkaError::Git(e))?;
    let remote_head = remote_branch.get();

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
        return attempt_merge(&repo, repo_name, local_oid, remote_oid, &branch);
    } else if ahead > 0 {
        // Local ahead - push
        let push_spec = format!("refs/heads/{}:refs/heads/{}", branch, branch);
        remote.push(&[&push_spec], None)
            .map_err(|e| GitkaError::Git(e))?;
        return Ok(SyncStatus::Ahead(ahead as u32));
    } else if behind > 0 {
        // Remote ahead - pull (fast-forward)
        let local_branch_name = format!("refs/heads/{}", branch);
        let upstream_name = format!("origin/{}", branch);
        let mut local_branch = repo.find_branch(&local_branch_name, git2::BranchType::Local)
            .or_else(|_| repo.find_branch(&branch, git2::BranchType::Local))
            .map_err(|e| GitkaError::Git(e))?;

        // Set upstream tracking
        let _ = local_branch.set_upstream(Some(&upstream_name));

        // Checkout the remote commit to fast-forward
        let remote_commit = repo.find_commit(remote_oid)
            .map_err(|e| GitkaError::Git(e))?;
        repo.checkout_tree(remote_commit.as_object(), None)
            .map_err(|e| GitkaError::Git(e))?;

        // Update the branch reference
        local_branch.get_mut().set_target(remote_oid, "fast-forward")
            .map_err(|e| GitkaError::Git(e))?;

        // Update HEAD to point to the branch
        repo.set_head(&local_branch_name)
            .map_err(|e| GitkaError::Git(e))?;

        return Ok(SyncStatus::Behind(behind as u32));
    }

    // Shouldn't reach here, but handle gracefully
    Ok(SyncStatus::InSync)
}

/// Attempt to merge diverged branches.
/// When there are no conflicts, performs the merge automatically and
/// creates a merge commit. When conflicts exist, returns Conflict status.
fn attempt_merge(
    repo: &git2::Repository,
    repo_name: &str,
    local_oid: git2::Oid,
    remote_oid: git2::Oid,
    branch: &str,
) -> Result<SyncStatus> {
    // Get the merge base (required for merge, but we also handle the no-base case)
    let _merge_base = repo.merge_base(local_oid, remote_oid)
        .map_err(|_| GitkaError::SyncConflict(format!("{}: no merge base found", repo_name)))?;

    // Get local and remote commits for merge
    let local_commit = repo.find_commit(local_oid)
        .map_err(|e| GitkaError::Git(e))?;
    let remote_commit = repo.find_commit(remote_oid)
        .map_err(|e| GitkaError::Git(e))?;

    // Try to merge the remote into local using merge_commits (takes &Commit)
    let mut merge_index = repo.merge_commits(
        &remote_commit,
        &local_commit,
        None,
    ).map_err(|e| GitkaError::Git(e))?;

    // Check for conflicts
    if merge_index.has_conflicts() {
        let conflicts: Vec<String> = merge_index.conflicts()
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

    // No conflicts — perform the merge automatically
    // 1. Write the merged tree from the index
    let tree_oid = merge_index.write_tree_to(repo)
        .map_err(|e| GitkaError::Git(e))?;
    let tree = repo.find_tree(tree_oid)
        .map_err(|e| GitkaError::Git(e))?;

    // 2. Get author/committer signature from config or use a fallback
    let signature = repo.signature()
        .unwrap_or_else(|_| {
            git2::Signature::now("Gitka Sync", "gitka@local")
                .expect("Failed to create fallback signature")
        });

    // 3. Determine the local branch reference name
    let local_ref = format!("refs/heads/{}", branch);

    // 4. Create the merge commit with two parents (local + remote)
    let parents = [&local_commit, &remote_commit];
    let merge_commit_oid = repo.commit(
        Some(&local_ref),
        &signature,
        &signature,
        &format!("Auto-merge branch '{}' of origin/{}", branch, branch),
        &tree,
        &parents,
    ).map_err(|e| GitkaError::Git(e))?;

    // 5. Checkout the merge commit
    let merge_commit = repo.find_commit(merge_commit_oid)
        .map_err(|e| GitkaError::Git(e))?;
    repo.checkout_tree(merge_commit.as_object(), None)
        .map_err(|e| GitkaError::Git(e))?;
    repo.set_head(&local_ref)
        .map_err(|e| GitkaError::Git(e))?;

    let (ahead, behind) = repo.graph_ahead_behind(local_oid, remote_oid)
        .map_err(|e| GitkaError::Git(e))?;

    tracing::info!(
        "Auto-merged {} (was {} ahead, {} behind)",
        repo_name, ahead, behind
    );

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

    let branch = default_branch(&repo);

    // Get local and remote HEADs
    let local_head = repo.head()
        .map_err(|e| GitkaError::Git(e))?;

    let remote_ref = format!("refs/remotes/origin/{}", branch);
    let remote_branch = match repo.find_branch(&remote_ref, git2::BranchType::Remote) {
        Ok(branch) => branch,
        Err(_) => {
            // If the specific branch doesn't exist, try origin/HEAD
            match repo.find_branch("origin/HEAD", git2::BranchType::Remote) {
                Ok(b) => b,
                Err(_) => {
                    // Fetch and try again
                    let mut remote = repo.find_remote("origin")
                        .map_err(|e| GitkaError::Git(e))?;
                    remote.fetch(&[] as &[&str], None, None)
                        .map_err(|e| GitkaError::Git(e))?;

                    repo.find_branch(&remote_ref, git2::BranchType::Remote)
                        .map_err(|e| GitkaError::Git(e))?
                }
            }
        }
    };

    let remote_head = remote_branch.get();

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

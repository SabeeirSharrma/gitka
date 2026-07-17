#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::config::SourceConfig;
use crate::error::{GitkaError, Result};

/// Information about a remote repository
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteRepo {
    /// Repository name
    pub name: String,
    /// Full name (owner/name)
    pub full_name: String,
    /// Clone URL
    pub clone_url: String,
    /// SSH URL (if available)
    pub ssh_url: Option<String>,
    /// Default branch
    pub default_branch: String,
    /// Description
    pub description: Option<String>,
    /// Whether the repo is private
    pub private: bool,
    /// Size in bytes
    pub size: u64,
}

/// GitHub API response for listing repos
#[derive(Debug, Deserialize)]
struct GitHubRepo {
    name: String,
    full_name: String,
    clone_url: String,
    ssh_url: Option<String>,
    default_branch: String,
    description: Option<String>,
    private: bool,
    size: u64,
}

/// Source provider trait
pub trait SourceProvider {
    /// List all repositories accessible by the user
    fn list_repos(&self) -> Result<Vec<RemoteRepo>>;

    /// Get clone URL for a specific repo
    fn get_clone_url(&self, repo_name: &str) -> Result<String>;

    /// Get the authentication method
    fn auth_method(&self) -> AuthMethod;
}

/// Authentication method for a source
#[derive(Debug, Clone)]
pub enum AuthMethod {
    /// No authentication (public repos)
    None,
    /// Personal access token (GitHub PAT)
    Token(String),
    /// SSH key
    SshKey(PathBuf),
}

/// GitHub source provider
pub struct GitHubSource {
    /// GitHub username or organization
    pub username: String,
    /// Personal access token (optional for public repos)
    pub token: Option<String>,
}

impl GitHubSource {
    pub fn new(username: String, token: Option<String>) -> Self {
        Self { username, token }
    }

    /// Create an authenticated HTTP client
    fn client(&self) -> Result<reqwest::blocking::Client> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Accept", "application/vnd.github.v3+json".parse().unwrap());
        headers.insert("User-Agent", "gitka/0.1.0".parse().unwrap());

        if let Some(token) = &self.token {
            headers.insert(
                "Authorization",
                format!("Bearer {}", token).parse().unwrap(),
            );
        }

        reqwest::blocking::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| GitkaError::Config(format!("Failed to create HTTP client: {}", e)))
    }
}

impl SourceProvider for GitHubSource {
    fn list_repos(&self) -> Result<Vec<RemoteRepo>> {
        let client = self.client()?;
        let url = format!("https://api.github.com/users/{}/repos", self.username);

        let response = client
            .get(&url)
            .query(&[("per_page", "100"), ("sort", "updated")])
            .send()
            .map_err(|e| GitkaError::Config(format!("GitHub API request failed: {}", e)))?;

        if response.status().as_u16() == 404 {
            return Err(GitkaError::Config(format!(
                "GitHub user or organization '{}' not found. Please check the username.",
                self.username
            )));
        }

        if !response.status().is_success() {
            return Err(GitkaError::Config(format!(
                "GitHub API error: {} - {}",
                response.status(),
                response.text().unwrap_or_else(|_| "Unknown error".to_string())
            )));
        }

        let repos: Vec<GitHubRepo> = response
            .json()
            .map_err(|e| GitkaError::Config(format!("Failed to parse GitHub response: {}", e)))?;

        Ok(repos
            .into_iter()
            .map(|r| RemoteRepo {
                name: r.name,
                full_name: r.full_name,
                clone_url: r.clone_url,
                ssh_url: r.ssh_url,
                default_branch: r.default_branch,
                description: r.description,
                private: r.private,
                size: r.size * 1024, // GitHub reports size in KB, convert to bytes
            })
            .collect())
    }

    fn get_clone_url(&self, repo_name: &str) -> Result<String> {
        if self.token.is_some() {
            // Use HTTPS with token for authentication
            Ok(format!(
                "https://x-access-token@github.com/{}/{}.git",
                self.username,
                repo_name
            ))
        } else {
            // Use public HTTPS URL
            Ok(format!(
                "https://github.com/{}/{}.git",
                self.username, repo_name
            ))
        }
    }

    fn auth_method(&self) -> AuthMethod {
        match &self.token {
            Some(token) => AuthMethod::Token(token.clone()),
            None => AuthMethod::None,
        }
    }
}

/// GitFlare source provider
pub struct GitFlareSource {
    /// GitFlare instance URL
    pub url: String,
    /// Authentication token
    pub token: Option<String>,
}

impl GitFlareSource {
    pub fn new(url: String, token: Option<String>) -> Self {
        Self { url, token }
    }

    /// Create an authenticated HTTP client
    fn client(&self) -> Result<reqwest::blocking::Client> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("User-Agent", "gitka/0.1.0".parse().unwrap());

        if let Some(token) = &self.token {
            headers.insert(
                "Authorization",
                format!("Bearer {}", token).parse().unwrap(),
            );
        }

        reqwest::blocking::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| GitkaError::Config(format!("Failed to create HTTP client: {}", e)))
    }
}

impl SourceProvider for GitFlareSource {
    fn list_repos(&self) -> Result<Vec<RemoteRepo>> {
        let client = self.client()?;
        let url = format!("{}/api/repos", self.url);

        let response = client
            .get(&url)
            .send()
            .map_err(|e| GitkaError::Config(format!("GitFlare API request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(GitkaError::Config(format!(
                "GitFlare API error: {}",
                response.status()
            )));
        }

        let repos: Vec<GitFlareRepo> = response
            .json()
            .map_err(|e| GitkaError::Config(format!("Failed to parse GitFlare response: {}", e)))?;

        Ok(repos
            .into_iter()
            .map(|r| RemoteRepo {
                name: r.name.clone(),
                full_name: r.name,
                clone_url: r.clone_url,
                ssh_url: None,
                default_branch: r.default_branch.unwrap_or_else(|| "main".to_string()),
                description: r.description,
                private: false,
                size: r.size,
            })
            .collect())
    }

    fn get_clone_url(&self, repo_name: &str) -> Result<String> {
        Ok(format!("{}/{}.git", self.url, repo_name))
    }

    fn auth_method(&self) -> AuthMethod {
        match &self.token {
            Some(token) => AuthMethod::Token(token.clone()),
            None => AuthMethod::None,
        }
    }
}

/// GitFlare API response for listing repos
#[derive(Debug, Deserialize)]
struct GitFlareRepo {
    name: String,
    clone_url: String,
    default_branch: Option<String>,
    description: Option<String>,
    size: u64,
}

/// Create a source provider from config
pub fn create_source(config: &SourceConfig) -> Result<Box<dyn SourceProvider>> {
    if let Some(username) = &config.github_username {
        return Ok(Box::new(GitHubSource::new(
            username.clone(),
            config.auth_token.clone(),
        )));
    }

    if let Some(url) = &config.gitflare_url {
        return Ok(Box::new(GitFlareSource::new(
            url.clone(),
            config.auth_token.clone(),
        )));
    }

    Err(GitkaError::Config(
        "No source configured. Set github_username or gitflare_url in config.".to_string(),
    ))
}

/// Clone a repository to the target path
pub fn clone_repo(
    repo: &RemoteRepo,
    target_dir: &Path,
    auth: &AuthMethod,
    _shallow: bool,
) -> Result<PathBuf> {
    let clone_url = match auth {
        AuthMethod::Token(token) => {
            // Insert token into URL for HTTPS authentication
            repo.clone_url
                .replace("https://", &format!("https://x-access-token:{}@", token))
        }
        _ => repo.clone_url.clone(),
    };

    let mut builder = git2::build::RepoBuilder::new();
    builder.bare(true);

    // Note: git2 doesn't support shallow clones directly via RepoBuilder
    // We'll clone full and could optionally prune history later

    let repo_path = target_dir.join(&repo.name);

    tracing::info!("Cloning {} to {}", repo.full_name, repo_path.display());

    builder
        .clone(&clone_url, &repo_path)
        .map_err(|e| GitkaError::Git(e))?;

    Ok(repo_path)
}

/// Fetch updates for an existing repository
pub fn fetch_repo(repo_path: &Path, auth: &AuthMethod) -> Result<()> {
    let repo = git2::Repository::open(repo_path).map_err(|e| GitkaError::Git(e))?;

    let mut remote = repo.find_remote("origin").map_err(|e| GitkaError::Git(e))?;

    // Update remote URL with authentication if needed
    if let AuthMethod::Token(token) = auth {
        let url = remote.url().unwrap_or("").to_string();
        let _authenticated_url = url
            .replace("https://", &format!("https://x-access-token:{}@", token));
        // Note: git2 Remote doesn't have set_url, so we'll use the URL with token directly
        // This is a limitation - we may need to use git2's credential callback instead
    }

    remote
        .fetch(&[] as &[&str], None, None)
        .map_err(|e| GitkaError::Git(e))?;

    Ok(())
}

/// Get the size of a repository (working directory)
pub fn repo_size(repo_path: &Path) -> Result<u64> {
    let mut total_size = 0u64;

    for entry in walkdir::WalkDir::new(repo_path) {
        let entry = entry.map_err(|e| GitkaError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        if entry.file_type().is_file() {
            let metadata = entry
                .metadata()
                .map_err(|e| GitkaError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
            total_size += metadata.len();
        }
    }

    Ok(total_size)
}

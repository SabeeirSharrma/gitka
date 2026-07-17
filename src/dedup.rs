#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::Config;
use crate::error::{GitkaError, Result};

/// Dedup marker byte: content that follows is a SHA256 reference
pub const DEDUP_MARKER: u8 = 0x01;
/// Raw content marker byte: content that follows is the actual file data
pub const RAW_MARKER: u8 = 0x00;

/// Reference to content stored in the dedup store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupRef {
    /// SHA256 hash of the content
    pub hash: String,
    /// Which archive part contains this blob (primary = first part)
    pub source_part: u32,
    /// Byte offset within the uncompressed stream of the source archive
    pub offset: u64,
    /// Content length in bytes
    pub length: u64,
}

/// Content-addressable dedup store shared across repos
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DedupIndex {
    /// SHA256 hash -> dedup reference
    pub entries: HashMap<String, DedupRef>,
    /// Total deduplicated bytes saved
    pub bytes_saved: u64,
}

/// Manages the dedup store on disk
pub struct DedupStore {
    /// Path to .gitka/dedup-store/
    store_path: PathBuf,
    /// In-memory index
    index: DedupIndex,
}

impl DedupStore {
    /// Create or open a dedup store for the given config
    pub fn open(config: &Config) -> Self {
        let store_path = config.state_dir().join("dedup-store");
        Self {
            store_path,
            index: DedupIndex::default(),
        }
    }

    /// Create a dedup store at a specific path
    pub fn at(path: PathBuf) -> Self {
        Self {
            store_path: path,
            index: DedupIndex::default(),
        }
    }

    /// Initialize the store directory
    pub fn init(&self) -> Result<()> {
        std::fs::create_dir_all(&self.store_path)?;
        Ok(())
    }

    /// Load index from disk
    pub fn load_index(&mut self) -> Result<()> {
        let index_path = self.store_path.join("index.toml");
        if index_path.exists() {
            let content = std::fs::read_to_string(&index_path)?;
            self.index = toml::from_str(&content)
                .map_err(|e| GitkaError::Config(format!("Failed to parse dedup index: {}", e)))?;
        }
        Ok(())
    }

    /// Save index to disk
    pub fn save_index(&self) -> Result<()> {
        std::fs::create_dir_all(&self.store_path)?;
        let index_path = self.store_path.join("index.toml");
        let content = toml::to_string_pretty(&self.index)
            .map_err(|e| GitkaError::Config(format!("Failed to serialize dedup index: {}", e)))?;
        std::fs::write(&index_path, content)?;
        Ok(())
    }

    /// Compute SHA256 hash of content
    pub fn hash_content(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    /// Look up content by hash
    pub fn lookup(&self, hash: &str) -> Option<&DedupRef> {
        self.index.entries.get(hash)
    }

    /// Register new content in the index
    pub fn register(&mut self, hash: String, ref_info: DedupRef) {
        self.index.bytes_saved += ref_info.length;
        self.index.entries.insert(hash, ref_info);
    }

    /// Check if content already exists
    pub fn contains(&self, hash: &str) -> bool {
        self.index.entries.contains_key(hash)
    }

    /// Get total bytes saved by dedup
    pub fn bytes_saved(&self) -> u64 {
        self.index.bytes_saved
    }

    /// Get number of deduped entries
    pub fn entry_count(&self) -> usize {
        self.index.entries.len()
    }

    /// Get stats for display
    pub fn stats(&self) -> DedupStats {
        DedupStats {
            entry_count: self.index.entries.len(),
            bytes_saved: self.index.bytes_saved,
        }
    }
}

/// Dedup statistics for display
#[derive(Debug)]
pub struct DedupStats {
    pub entry_count: usize,
    pub bytes_saved: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_hash_consistency() {
        let data = b"hello world";
        let hash1 = DedupStore::hash_content(data);
        let hash2 = DedupStore::hash_content(data);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_dedup_store_roundtrip() {
        let temp = TempDir::new().unwrap();
        let mut store = DedupStore::at(temp.path().to_path_buf());
        store.init().unwrap();
        store.load_index().unwrap();

        let hash = DedupStore::hash_content(b"test content");
        let ref_info = DedupRef {
            hash: hash.clone(),
            source_part: 0,
            offset: 0,
            length: 12,
        };

        assert!(!store.contains(&hash));
        store.register(hash.clone(), ref_info);
        assert!(store.contains(&hash));
        assert_eq!(store.entry_count(), 1);
        assert_eq!(store.bytes_saved(), 12);

        store.save_index().unwrap();

        // Reload and verify
        let mut store2 = DedupStore::at(temp.path().to_path_buf());
        store2.load_index().unwrap();
        assert!(store2.contains(&hash));
        assert_eq!(store2.entry_count(), 1);
    }
}

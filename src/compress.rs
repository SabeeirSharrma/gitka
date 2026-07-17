#![allow(dead_code)]

use std::path::Path;

use crate::config::{CompressionConfig, CompressionTier};
use crate::error::{GitkaError, Result};

/// Compression tier thresholds based on free space vs needed space
pub struct BudgetCheck {
    /// Free space on target drive in bytes
    pub free_space: u64,
    /// Total size of repos to compress in bytes
    pub needed_space: u64,
}

impl BudgetCheck {
    pub fn new(free_space: u64, needed_space: u64) -> Self {
        Self {
            free_space,
            needed_space,
        }
    }

    /// Determine the appropriate compression tier based on budget
    pub fn determine_tier(&self, config: &CompressionConfig) -> CompressionTier {
        match &config.tier {
            CompressionTier::Auto => {
                let ratio = self.free_space as f64 / self.needed_space as f64;
                if ratio >= 3.0 {
                    CompressionTier::Low
                } else if ratio >= 1.0 {
                    CompressionTier::Medium
                } else {
                    CompressionTier::High
                }
            }
            tier => tier.clone(),
        }
    }

    /// Check if the budget is exceeded even at max compression
    pub fn is_over_budget(&self) -> bool {
        // Even at maximum compression (roughly 4:1 for code), if we can't fit, it's over budget
        // This is a conservative estimate; actual compression ratios vary
        self.free_space < self.needed_space / 4
    }

    /// Get the estimated compression ratio for a tier
    pub fn compression_ratio(&self, tier: &CompressionTier) -> f64 {
        match tier {
            CompressionTier::Low => 2.0,    // zstd -3 to -9
            CompressionTier::Medium => 3.0,  // zstd -15 to -19
            CompressionTier::High => 4.0,    // zstd --ultra -22
            CompressionTier::Auto => 3.0,    // default to medium
        }
    }
}

/// Get zstd compression level for a tier
pub fn level_for_tier(tier: &CompressionTier) -> i32 {
    match tier {
        CompressionTier::Low => 6,      // zstd default
        CompressionTier::Medium => 15,  // zstd -15
        CompressionTier::High => 22,    // zstd --ultra -22
        CompressionTier::Auto => 6,     // will be overridden
    }
}

/// Compress a directory to a zstd archive
pub fn compress_directory(
    source_dir: &Path,
    archive_path: &Path,
    config: &CompressionConfig,
) -> Result<u64> {
    use std::fs::File;
    use std::io::{Read, Write};
    use zstd::stream::write::Encoder;

    let tier = config.tier.clone();
    let level = level_for_tier(&tier);

    // Create the archive file
    let file = File::create(archive_path)
        .map_err(|e| GitkaError::Compression(format!("Failed to create archive: {}", e)))?;

    // Create zstd encoder
    let mut encoder = Encoder::new(file, level)
        .map_err(|e| GitkaError::Compression(format!("Failed to create encoder: {}", e)))?;

    // Walk the directory and compress files
    for entry in walkdir::WalkDir::new(source_dir) {
        let entry = entry.map_err(|e| GitkaError::Compression(format!("Walk error: {}", e)))?;
        let path = entry.path();

        if path.is_file() {
            let mut file = File::open(path)
                .map_err(|e| GitkaError::Compression(format!("Failed to open {}: {}", path.display(), e)))?;

            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)
                .map_err(|e| GitkaError::Compression(format!("Read error: {}", e)))?;

            // Write file header (relative path length + path)
            let relative = path.strip_prefix(source_dir).unwrap();
            let relative_bytes = relative.to_string_lossy().as_bytes().to_vec();
            let len = relative_bytes.len() as u32;
            encoder.write_all(&len.to_le_bytes())
                .map_err(|e| GitkaError::Compression(format!("Write error: {}", e)))?;
            encoder.write_all(&relative_bytes)
                .map_err(|e| GitkaError::Compression(format!("Write error: {}", e)))?;

            // Write file content length + content
            let content_len = buffer.len() as u64;
            encoder.write_all(&content_len.to_le_bytes())
                .map_err(|e| GitkaError::Compression(format!("Write error: {}", e)))?;
            encoder.write_all(&buffer)
                .map_err(|e| GitkaError::Compression(format!("Write error: {}", e)))?;
        }
    }

    // Finish encoding
    encoder.finish()
        .map_err(|e| GitkaError::Compression(format!("Failed to finish encoding: {}", e)))?;

    // Get the final archive size
    let archive_size = std::fs::metadata(archive_path)
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(archive_size)
}

/// Decompress a zstd archive to a directory
pub fn decompress_directory(
    archive_path: &Path,
    target_dir: &Path,
) -> Result<u64> {
    use std::fs::File;
    use std::io::Read;
    use zstd::stream::read::Decoder;

    let file = File::open(archive_path)
        .map_err(|e| GitkaError::Extraction(format!("Failed to open archive: {}", e)))?;

    let mut decoder = Decoder::new(file)
        .map_err(|e| GitkaError::Extraction(format!("Failed to create decoder: {}", e)))?;

    let mut total_bytes = 0u64;

    // Read and extract files
    loop {
        // Read file path length
        let mut len_buf = [0u8; 4];
        match decoder.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(GitkaError::Extraction(format!("Read error: {}", e))),
        }
        let path_len = u32::from_le_bytes(len_buf) as usize;

        // Read file path
        let mut path_buf = vec![0u8; path_len];
        decoder.read_exact(&mut path_buf)
            .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
        let relative_path = String::from_utf8(path_buf)
            .map_err(|e| GitkaError::Extraction(format!("Invalid UTF-8: {}", e)))?;

        // Read content length
        let mut content_len_buf = [0u8; 8];
        decoder.read_exact(&mut content_len_buf)
            .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
        let content_len = u64::from_le_bytes(content_len_buf);

        // Read content
        let mut content = vec![0u8; content_len as usize];
        decoder.read_exact(&mut content)
            .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;

        // Write to target
        let target_path = target_dir.join(&relative_path);
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| GitkaError::Extraction(format!("Create dir error: {}", e)))?;
        }
        std::fs::write(&target_path, &content)
            .map_err(|e| GitkaError::Extraction(format!("Write error: {}", e)))?;

        total_bytes += content_len;
    }

    Ok(total_bytes)
}

/// Verify archive integrity
pub fn verify_archive(archive_path: &Path) -> Result<()> {
    use std::fs::File;
    use std::io::Read;
    use zstd::stream::read::Decoder;

    let file = File::open(archive_path)
        .map_err(|e| GitkaError::VerificationFailed(
            archive_path.display().to_string(),
            format!("Failed to open: {}", e),
        ))?;

    let mut decoder = Decoder::new(file)
        .map_err(|e| GitkaError::VerificationFailed(
            archive_path.display().to_string(),
            format!("Failed to create decoder: {}", e),
        ))?;

    // Try to decompress the entire archive to verify integrity
    let mut buffer = Vec::new();
    decoder.read_to_end(&mut buffer)
        .map_err(|e| GitkaError::VerificationFailed(
            archive_path.display().to_string(),
            format!("Decompression failed: {}", e),
        ))?;

    Ok(())
}

/// Calculate SHA256 hash of a file
pub fn calculate_hash(file_path: &Path) -> Result<String> {
    use sha2::{Sha256, Digest};
    use std::fs::File;
    use std::io::Read;

    let mut file = File::open(file_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

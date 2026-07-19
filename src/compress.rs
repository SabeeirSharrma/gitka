#![allow(dead_code)]

use std::path::Path;
use std::io::{Read, Seek, SeekFrom};

use crate::archive::{ArchiveHeader, COMPRESSION_ZSTD};
use crate::config::{CompressionConfig, CompressionTier};
use crate::dedup::{DedupRef, DedupStore, DEDUP_MARKER, RAW_MARKER};
use crate::error::{GitkaError, Result};
use zstd::stream::read::Decoder;

/// Size of the archive header in bytes
const HEADER_SIZE: usize = 12;

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
                if self.needed_space == 0 {
                    return CompressionTier::Low;
                }
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
        if self.needed_space == 0 {
            return false;
        }

        self.free_space < self.needed_space
    }

    /// Get the estimated compression ratio for a tier
    pub fn compression_ratio(&self, tier: &CompressionTier) -> f64 {
        match tier {
            CompressionTier::Low => 2.0,
            CompressionTier::Medium => 3.0,
            CompressionTier::High => 4.0,
            CompressionTier::Auto => 3.0,
        }
    }
}

/// Get zstd compression level for a tier
pub fn level_for_tier(tier: &CompressionTier) -> i32 {
    match tier {
        CompressionTier::Low => 6,
        CompressionTier::Medium => 15,
        CompressionTier::High => 22,
        CompressionTier::Auto => 6,
    }
}

/// Result of a compression operation
pub struct CompressResult {
    /// Total compressed bytes written
    pub total_size: u64,
    /// Number of volume parts created (1 = no splitting)
    pub volume_count: u32,
    /// Names of all part files (e.g., ["repo.gitka.zst", "repo.gitka.zst.002"])
    pub part_files: Vec<String>,
    /// Dedup stats (if dedup was enabled)
    pub dedup_bytes_saved: u64,
    /// Number of files deduped
    pub dedup_files_skipped: u64,
}

/// Compress a directory to a zstd archive, with optional volume splitting and dedup
pub fn compress_directory(
    source_dir: &Path,
    archive_path: &Path,
    config: &CompressionConfig,
) -> Result<u64> {
    // Legacy single-file interface — delegates to compress_directory_with_options
    let result = compress_directory_with_options(source_dir, archive_path, config, None)?;
    Ok(result.total_size)
}

/// Train a zstd dictionary from sample files in a directory
pub fn train_dictionary(source_dir: &Path, dict_size_mb: u32) -> Result<Vec<u8>> {
    use std::fs::File;
    use std::io::Read;

    let max_dict_size = (dict_size_mb as usize) * 1024 * 1024;

    // Collect sample file contents for dictionary training
    let mut samples: Vec<Vec<u8>> = Vec::new();
    let mut total_sample_bytes: usize = 0;
    let max_sample_bytes = max_dict_size * 4; // Sample up to 4x dict size

    for entry in walkdir::WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        // Skip very large files (>10MB) and binary files
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if metadata.len() > 10 * 1024 * 1024 || metadata.len() == 0 {
            continue;
        }

        let mut file = match File::open(path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let mut buf = Vec::new();
        if file.read_to_end(&mut buf).is_err() {
            continue;
        }

        total_sample_bytes += buf.len();
        samples.push(buf);

        if total_sample_bytes >= max_sample_bytes {
            break;
        }
    }

    if samples.is_empty() {
        return Err(GitkaError::Compression(
            "No suitable files found for dictionary training".to_string(),
        ));
    }

    // Train the dictionary
    let dict = zstd::dict::from_samples(&samples, max_dict_size)
        .map_err(|e| GitkaError::Compression(format!("Dictionary training failed: {}", e)))?;

    Ok(dict)
}

/// Load a trained dictionary from disk
pub fn load_dictionary(dict_path: &Path) -> Option<Vec<u8>> {
    std::fs::read(dict_path).ok()
}

/// Compress with full options (volume splitting, dedup)
pub fn compress_directory_with_options(
    source_dir: &Path,
    archive_path: &Path,
    config: &CompressionConfig,
    mut dedup_store: Option<&mut DedupStore>,
) -> Result<CompressResult> {
    use indicatif::{ProgressBar, ProgressStyle};
    use std::fs::File;
    use std::io::{Read, Write};
    use zstd::stream::write::Encoder;

    let tier = config.tier.clone();
    let level = level_for_tier(&tier);

    // Determine split size
    let split_bytes = config.volume_splitting.as_ref().map(|vs| vs.size_mb * 1024 * 1024);

    // Load zstd dictionary if available
    let dict_path = archive_path.parent()
        .unwrap_or(Path::new("."))
        .join(".trained.dict");
    let dictionary = load_dictionary(&dict_path);

    // Count files for progress bar
    let file_count = walkdir::WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .count() as u64;

    let pb = ProgressBar::new(file_count);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} files ({eta})")
        .unwrap()
        .progress_chars("#>-"));

    // Create the first part file and write archive header
    let mut part_number: u32 = 1;
    let mut current_part_path = archive_path.to_path_buf();
    let mut file = File::create(&current_part_path)
        .map_err(|e| GitkaError::Compression(format!("Failed to create archive: {}", e)))?;

    // Write the 12-byte archive header (GITKA magic + version + method)
    let header = ArchiveHeader::new(COMPRESSION_ZSTD);
    file.write_all(&header.to_bytes())
        .map_err(|e| GitkaError::Compression(format!("Failed to write archive header: {}", e)))?;

    let mut encoder = match &dictionary {
        Some(dict) => Encoder::with_dictionary(file, level, dict)
            .map_err(|e| GitkaError::Compression(format!("Failed to create encoder with dictionary: {}", e)))?,
        None => Encoder::new(file, level)
            .map_err(|e| GitkaError::Compression(format!("Failed to create encoder: {}", e)))?,
    };
    let mut bytes_in_current_part: u64 = 0;
    let mut part_files: Vec<String> = vec![current_part_path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()];

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

            let relative = path.strip_prefix(source_dir).unwrap();
            let relative_bytes = relative.to_string_lossy().as_bytes().to_vec();

            // Dedup: check if content already exists
            if let Some(store) = dedup_store.as_ref() {
                let content_hash = DedupStore::hash_content(&buffer);
                if store.contains(&content_hash) {
                    // Write dedup reference: [path_len: u32][path][MARKER: u8][hash_len: u32][hash: bytes]
                    let path_len = relative_bytes.len() as u32;
                    encoder.write_all(&path_len.to_le_bytes())
                        .map_err(|e| GitkaError::Compression(format!("Write error: {}", e)))?;
                    encoder.write_all(&relative_bytes)
                        .map_err(|e| GitkaError::Compression(format!("Write error: {}", e)))?;
                    // Dedup marker
                    encoder.write_all(&[DEDUP_MARKER])
                        .map_err(|e| GitkaError::Compression(format!("Write error: {}", e)))?;
                    // Hash
                    let hash_bytes = content_hash.as_bytes();
                    let hash_len = hash_bytes.len() as u32;
                    encoder.write_all(&hash_len.to_le_bytes())
                        .map_err(|e| GitkaError::Compression(format!("Write error: {}", e)))?;
                    encoder.write_all(hash_bytes)
                        .map_err(|e| GitkaError::Compression(format!("Write error: {}", e)))?;
                    // Placeholder content_len = 0 (used for dedup marker)
                    encoder.write_all(&0u64.to_le_bytes())
                        .map_err(|e| GitkaError::Compression(format!("Write error: {}", e)))?;
                    // No actual content follows — hash is the reference

                    if let Some(store) = dedup_store.as_mut() {
                        store.record_saved_bytes(buffer.len() as u64);
                    }

                    pb.inc(1);
                    continue;
                }
            }

            // Write raw content: [path_len: u32][path][RAW_MARKER: u8][content_len: u64][content]
            let path_len = relative_bytes.len() as u32;
            encoder.write_all(&path_len.to_le_bytes())
                .map_err(|e| GitkaError::Compression(format!("Write error: {}", e)))?;
            encoder.write_all(&relative_bytes)
                .map_err(|e| GitkaError::Compression(format!("Write error: {}", e)))?;
            // Raw marker
            encoder.write_all(&[RAW_MARKER])
                .map_err(|e| GitkaError::Compression(format!("Write error: {}", e)))?;
            // Content
            let content_len = buffer.len() as u64;
            encoder.write_all(&content_len.to_le_bytes())
                .map_err(|e| GitkaError::Compression(format!("Write error: {}", e)))?;
            encoder.write_all(&buffer)
                .map_err(|e| GitkaError::Compression(format!("Write error: {}", e)))?;

            // Register in dedup store
            if let Some(store) = dedup_store.as_mut() {
                let content_hash = DedupStore::hash_content(&buffer);
                if !store.contains(&content_hash) {
                    store.register(content_hash.clone(), DedupRef {
                        hash: content_hash,
                        source_part: part_number,
                        offset: bytes_in_current_part,
                        length: buffer.len() as u64,
                    });
                }
            }

            // Estimate bytes written (uncompressed frame size)
            let frame_size = 4 + relative_bytes.len() + 1 + 8 + buffer.len();
            bytes_in_current_part += frame_size as u64;

            pb.inc(1);

            // Volume splitting: check if we should split
            if let Some(max_bytes) = split_bytes {
                if bytes_in_current_part >= max_bytes {
                    // Finalize current part
                    encoder.finish()
                        .map_err(|e| GitkaError::Compression(format!("Failed to finish part {}: {}", part_number, e)))?;

                    // Start new part
                    part_number += 1;
                    let new_part_name = format!("{}.{}", archive_path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy(), format!("{:03}", part_number));
                    current_part_path = archive_path.parent()
                        .unwrap_or(Path::new("."))
                        .join(&new_part_name);

                    file = File::create(&current_part_path)
                        .map_err(|e| GitkaError::Compression(format!("Failed to create part {}: {}", part_number, e)))?;
                    encoder = match &dictionary {
                        Some(dict) => Encoder::with_dictionary(file, level, dict)
                            .map_err(|e| GitkaError::Compression(format!("Failed to create encoder for part {}: {}", part_number, e)))?,
                        None => Encoder::new(file, level)
                            .map_err(|e| GitkaError::Compression(format!("Failed to create encoder for part {}: {}", part_number, e)))?,
                    };
                    bytes_in_current_part = 0;
                    part_files.push(new_part_name);
                }
            }
        }
    }

    pb.finish_with_message("done");

    // Finish encoding
    encoder.finish()
        .map_err(|e| GitkaError::Compression(format!("Failed to finish encoding: {}", e)))?;

    // Get total compressed size across all parts
    let mut total_compressed_size: u64 = 0;
    for name in &part_files {
        let part_path = archive_path.parent()
            .unwrap_or(Path::new("."))
            .join(name);
        total_compressed_size += std::fs::metadata(&part_path)
            .map(|m| m.len())
            .unwrap_or(0);
    }

    Ok(CompressResult {
        total_size: total_compressed_size,
        volume_count: part_number,
        part_files,
        dedup_bytes_saved: dedup_store.map(|s| s.bytes_saved()).unwrap_or(0),
        dedup_files_skipped: 0, // caller can compute from dedup stats
    })
}

/// Decompress a zstd archive to a directory (handles multi-volume)
pub fn decompress_directory(
    archive_path: &Path,
    target_dir: &Path,
) -> Result<u64> {
    use indicatif::{ProgressBar, ProgressStyle};

    // Detect multi-volume: check if .002, .003 etc. exist
    let parts = find_archive_parts(archive_path)?;

    // Calculate total compressed size for progress bar
    let mut total_compressed_size: u64 = 0;
    for part_name in &parts {
        let part_path = archive_path.parent()
            .unwrap_or(Path::new("."))
            .join(part_name);
        total_compressed_size += std::fs::metadata(&part_path)
            .map(|m| m.len())
            .unwrap_or(0);
    }

    let pb = ProgressBar::new(total_compressed_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec})")
        .unwrap()
        .progress_chars("#>-"));

    let mut total_bytes = 0u64;

    let first_part_path = archive_path.parent()
        .unwrap_or(Path::new("."))
        .join(&parts[0]);
    let file = open_archive_file(&first_part_path)?;
    let mut decoder = Decoder::new(file)
        .map_err(|e| GitkaError::Extraction(format!("Failed to create decoder: {}", e)))?;

    // Track which part we're reading from (for multi-volume)
    let mut _current_part_idx = 0;
    let mut remaining_parts = parts[1..].iter();

    // Read and extract files
    loop {
        // Read path length (4 bytes)
        let mut len_buf = [0u8; 4];
        match decoder.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Try next volume part
                if let Some(next_part_name) = remaining_parts.next() {
                    _current_part_idx += 1;
                    let next_part_path = archive_path.parent()
                        .unwrap_or(Path::new("."))
                        .join(next_part_name);
                    let file = open_archive_file(&next_part_path)?;
                    decoder = Decoder::new(file)
                        .map_err(|e| GitkaError::Extraction(format!("Failed to create decoder for part {}: {}", next_part_name, e)))?;
                    pb.inc(0); // update progress
                    continue;
                }
                break; // No more parts
            }
            Err(e) => return Err(GitkaError::Extraction(format!("Read error: {}", e))),
        }
        let path_len = u32::from_le_bytes(len_buf) as usize;
        pb.inc(4);

        // Read file path
        let mut path_buf = vec![0u8; path_len];
        decoder.read_exact(&mut path_buf)
            .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
        let relative_path = String::from_utf8(path_buf)
            .map_err(|e| GitkaError::Extraction(format!("Invalid UTF-8: {}", e)))?;
        pb.inc(path_len as u64);

        // Read marker byte (0x00 = raw, 0x01 = dedup ref)
        let mut marker_buf = [0u8; 1];
        decoder.read_exact(&mut marker_buf)
            .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
        pb.inc(1);

        if marker_buf[0] == DEDUP_MARKER {
            // Dedup reference: read hash_len + hash + placeholder content_len
            let mut hash_len_buf = [0u8; 4];
            decoder.read_exact(&mut hash_len_buf)
                .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
            let hash_len = u32::from_le_bytes(hash_len_buf) as usize;
            pb.inc(4);

            let mut hash_buf = vec![0u8; hash_len];
            decoder.read_exact(&mut hash_buf)
                .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
            let _hash = String::from_utf8(hash_buf)
                .map_err(|e| GitkaError::Extraction(format!("Invalid hash: {}", e)))?;
            pb.inc(hash_len as u64);

            // Read placeholder content_len (should be 0)
            let mut placeholder_len_buf = [0u8; 8];
            decoder.read_exact(&mut placeholder_len_buf)
                .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
            pb.inc(8);

            // Dedup reference — file will be resolved later by caller
            // For now, write an empty placeholder file
            let target_path = target_dir.join(&relative_path);
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| GitkaError::Extraction(format!("Create dir error: {}", e)))?;
            }
            // Write empty placeholder — caller with dedup store can resolve
            std::fs::write(&target_path, &[])
                .map_err(|e| GitkaError::Extraction(format!("Write error: {}", e)))?;
        } else {
            // Raw content: read content_len + content
            let mut content_len_buf = [0u8; 8];
            decoder.read_exact(&mut content_len_buf)
                .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
            let content_len = u64::from_le_bytes(content_len_buf);
            pb.inc(8);

            let mut content = vec![0u8; content_len as usize];
            decoder.read_exact(&mut content)
                .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
            pb.inc(content_len);

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
    }

    pb.finish_with_message("done");
    Ok(total_bytes)
}

/// Decompress with dedup store resolution
pub fn decompress_directory_with_dedup(
    archive_path: &Path,
    target_dir: &Path,
    dedup_store: &DedupStore,
) -> Result<u64> {
    use indicatif::{ProgressBar, ProgressStyle};

    let parts = find_archive_parts(archive_path)?;

    let mut total_compressed_size: u64 = 0;
    for part_name in &parts {
        let part_path = archive_path.parent()
            .unwrap_or(Path::new("."))
            .join(part_name);
        total_compressed_size += std::fs::metadata(&part_path)
            .map(|m| m.len())
            .unwrap_or(0);
    }

    let pb = ProgressBar::new(total_compressed_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec})")
        .unwrap()
        .progress_chars("#>-"));

    let mut total_bytes = 0u64;

    let first_part_path = archive_path.parent()
        .unwrap_or(Path::new("."))
        .join(&parts[0]);
    let file = open_archive_file(&first_part_path)?;
    let mut decoder = Decoder::new(file)
        .map_err(|e| GitkaError::Extraction(format!("Failed to create decoder: {}", e)))?;

    let mut remaining_parts = parts[1..].iter();

    loop {
        let mut len_buf = [0u8; 4];
        match decoder.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                if let Some(next_part_name) = remaining_parts.next() {
                    let next_part_path = archive_path.parent()
                        .unwrap_or(Path::new("."))
                        .join(next_part_name);
                    let file = open_archive_file(&next_part_path)?;
                    decoder = Decoder::new(file)
                        .map_err(|e| GitkaError::Extraction(format!("Failed to create decoder for part {}: {}", next_part_name, e)))?;
                    pb.inc(0);
                    continue;
                }
                break;
            }
            Err(e) => return Err(GitkaError::Extraction(format!("Read error: {}", e))),
        }
        let path_len = u32::from_le_bytes(len_buf) as usize;
        pb.inc(4);

        let mut path_buf = vec![0u8; path_len];
        decoder.read_exact(&mut path_buf)
            .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
        let relative_path = String::from_utf8(path_buf)
            .map_err(|e| GitkaError::Extraction(format!("Invalid UTF-8: {}", e)))?;
        pb.inc(path_len as u64);

        let mut marker_buf = [0u8; 1];
        decoder.read_exact(&mut marker_buf)
            .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
        pb.inc(1);

        if marker_buf[0] == DEDUP_MARKER {
            let mut hash_len_buf = [0u8; 4];
            decoder.read_exact(&mut hash_len_buf)
                .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
            let hash_len = u32::from_le_bytes(hash_len_buf) as usize;
            pb.inc(4);

            let mut hash_buf = vec![0u8; hash_len];
            decoder.read_exact(&mut hash_buf)
                .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
            let hash = String::from_utf8(hash_buf)
                .map_err(|e| GitkaError::Extraction(format!("Invalid hash: {}", e)))?;
            pb.inc(hash_len as u64);

            let mut placeholder_len_buf = [0u8; 8];
            decoder.read_exact(&mut placeholder_len_buf)
                .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
            pb.inc(8);

            // Resolve dedup reference
            if let Some(dedup_ref) = dedup_store.lookup(&hash) {
                // Read the original content from the source archive part
                let source_part_name = format!("{}.{:03}",
                    archive_path.file_name().unwrap_or_default().to_string_lossy(),
                    dedup_ref.source_part);
                let source_path = if dedup_ref.source_part == 1 {
                    archive_path.to_path_buf()
                } else {
                    archive_path.parent()
                        .unwrap_or(Path::new("."))
                        .join(&source_part_name)
                };

                // Read the raw content from the source archive
                let source_content = read_raw_content_from_archive(&source_path, dedup_ref.offset, dedup_ref.length)?;

                let target_path = target_dir.join(&relative_path);
                if let Some(parent) = target_path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| GitkaError::Extraction(format!("Create dir error: {}", e)))?;
                }
                std::fs::write(&target_path, &source_content)
                    .map_err(|e| GitkaError::Extraction(format!("Write error: {}", e)))?;
                total_bytes += dedup_ref.length;
            } else {
                // Fallback: write empty placeholder
                let target_path = target_dir.join(&relative_path);
                if let Some(parent) = target_path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| GitkaError::Extraction(format!("Create dir error: {}", e)))?;
                }
                std::fs::write(&target_path, &[])
                    .map_err(|e| GitkaError::Extraction(format!("Write error: {}", e)))?;
            }
        } else {
            let mut content_len_buf = [0u8; 8];
            decoder.read_exact(&mut content_len_buf)
                .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
            let content_len = u64::from_le_bytes(content_len_buf);
            pb.inc(8);

            let mut content = vec![0u8; content_len as usize];
            decoder.read_exact(&mut content)
                .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
            pb.inc(content_len);

            let target_path = target_dir.join(&relative_path);
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| GitkaError::Extraction(format!("Create dir error: {}", e)))?;
            }
            std::fs::write(&target_path, &content)
                .map_err(|e| GitkaError::Extraction(format!("Write error: {}", e)))?;
            total_bytes += content_len;
        }
    }

    pb.finish_with_message("done");
    Ok(total_bytes)
}

/// Read raw content from a compressed archive at a given offset (for dedup resolution)
fn read_raw_content_from_archive(
    archive_path: &Path,
    offset: u64,
    length: u64,
) -> Result<Vec<u8>> {
    let file = open_archive_file(archive_path)?;
    let mut decoder = Decoder::new(file)
        .map_err(|e| GitkaError::Extraction(format!("Failed to create decoder: {}", e)))?;
    let mut consumed = 0u64;

    loop {
        let frame_start = consumed;
        let mut len_buf = [0u8; 4];
        match decoder.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(GitkaError::Extraction(format!("Read error: {}", e))),
        }
        let path_len = u32::from_le_bytes(len_buf) as u64;
        consumed += 4;

        let mut path_buf = vec![0u8; path_len as usize];
        decoder.read_exact(&mut path_buf)
            .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
        consumed += path_len;

        let mut marker_buf = [0u8; 1];
        decoder.read_exact(&mut marker_buf)
            .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
        consumed += 1;

        if marker_buf[0] == DEDUP_MARKER {
            let mut hash_len_buf = [0u8; 4];
            decoder.read_exact(&mut hash_len_buf)
                .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
            let hash_len = u32::from_le_bytes(hash_len_buf) as u64;
            consumed += 4;

            let mut hash_buf = vec![0u8; hash_len as usize];
            decoder.read_exact(&mut hash_buf)
                .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
            consumed += hash_len;

            let mut placeholder_len_buf = [0u8; 8];
            decoder.read_exact(&mut placeholder_len_buf)
                .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
            consumed += 8;
        } else {
            let mut content_len_buf = [0u8; 8];
            decoder.read_exact(&mut content_len_buf)
                .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
            let content_len = u64::from_le_bytes(content_len_buf);
            consumed += 8;

            if frame_start == offset {
                if content_len != length {
                    return Err(GitkaError::Extraction(format!(
                        "Dedup reference length mismatch: expected {}, found {}",
                        length, content_len
                    )));
                }

                let mut content = vec![0u8; content_len as usize];
                decoder.read_exact(&mut content)
                    .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
                return Ok(content);
            }

            let mut sink = vec![0u8; content_len as usize];
            decoder.read_exact(&mut sink)
                .map_err(|e| GitkaError::Extraction(format!("Read error: {}", e)))?;
            consumed += content_len;
        }
    }

    Err(GitkaError::Extraction(format!(
        "Unable to locate dedup source content at offset {} in {}",
        offset,
        archive_path.display()
    )))
}

/// Open a decoder for an archive part, rewinding if there is no custom Gitka header.
fn open_archive_file(part_path: &Path) -> Result<std::fs::File> {
    let mut file = std::fs::File::open(part_path)
        .map_err(|e| GitkaError::Extraction(format!("Failed to open archive: {}", e)))?;

    let mut header_buf = [0u8; HEADER_SIZE];
    let has_header = match file.read_exact(&mut header_buf) {
        Ok(()) => header_buf[0..5] == *crate::archive::ARCHIVE_MAGIC,
        Err(_) => false,
    };

    if has_header {
        let header = ArchiveHeader::from_bytes(&header_buf)?;
        header.validate()?;
    } else {
        file.seek(SeekFrom::Start(0))
            .map_err(|e| GitkaError::Extraction(format!("Failed to rewind archive: {}", e)))?;
    }

    Ok(file)
}

/// Find all archive parts in order
fn find_archive_parts(archive_path: &Path) -> Result<Vec<String>> {
    let base_name = archive_path.file_name()
        .ok_or_else(|| GitkaError::Extraction("Invalid archive path".to_string()))?
        .to_string_lossy()
        .to_string();

    let mut parts = vec![base_name.clone()];

    // Check for .002, .003, etc.
    let parent = archive_path.parent().unwrap_or(Path::new("."));
    let mut part_num: u32 = 2;
    loop {
        let part_name = format!("{}.{:03}", base_name, part_num);
        let part_path = parent.join(&part_name);
        if part_path.exists() {
            parts.push(part_name);
            part_num += 1;
        } else {
            break;
        }
    }

    Ok(parts)
}

/// Verify archive integrity (supports multi-volume)
pub fn verify_archive(archive_path: &Path) -> Result<()> {
    let parts = find_archive_parts(archive_path)?;

    for part_name in parts.iter() {
        let part_path = archive_path.parent()
            .unwrap_or(Path::new("."))
            .join(part_name);

        let file = open_archive_file(&part_path)
            .map_err(|e| GitkaError::VerificationFailed(
                part_path.display().to_string(),
                e.to_string(),
            ))?;
        let mut decoder = Decoder::new(file)
            .map_err(|e| GitkaError::VerificationFailed(
                part_path.display().to_string(),
                format!("Failed to create decoder: {}", e),
            ))?;

        let mut buffer = Vec::new();
        decoder.read_to_end(&mut buffer)
            .map_err(|e| GitkaError::VerificationFailed(
                part_path.display().to_string(),
                format!("Decompression failed: {}", e),
            ))?;
    }

    Ok(())
}

/// Calculate total compressed size across all volume parts
pub fn total_archive_size(archive_path: &Path) -> Result<u64> {
    let parts = find_archive_parts(archive_path)?;
    let mut total: u64 = 0;
    let parent = archive_path.parent().unwrap_or(Path::new("."));

    for part_name in &parts {
        let part_path = parent.join(part_name);
        total += std::fs::metadata(&part_path)
            .map(|m| m.len())
            .unwrap_or(0);
    }

    Ok(total)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use tempfile::TempDir;

    #[test]
    fn test_compress_decompress_roundtrip() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        // Create test files
        std::fs::write(source.path().join("file1.txt"), b"hello world").unwrap();
        std::fs::create_dir_all(source.path().join("subdir")).unwrap();
        std::fs::write(source.path().join("subdir/file2.txt"), b"nested file").unwrap();

        let config = Config::default().compression;
        let archive_path = target.path().join("test.gitka.zst");

        // Compress
        let size = compress_directory(source.path(), &archive_path, &config).unwrap();
        assert!(size > 0);

        // Decompress
        let extract_dir = target.path().join("extract");
        std::fs::create_dir_all(&extract_dir).unwrap();
        let bytes = decompress_directory(&archive_path, &extract_dir).unwrap();
        assert!(bytes > 0);

        // Verify content
        assert_eq!(
            std::fs::read(extract_dir.join("file1.txt")).unwrap(),
            b"hello world"
        );
        assert_eq!(
            std::fs::read(extract_dir.join("subdir/file2.txt")).unwrap(),
            b"nested file"
        );
    }
}

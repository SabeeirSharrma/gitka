use std::path::{Path, PathBuf};

use crate::config::CompressionBackend;
use crate::error::{GitkaError, Result};

/// Archive file magic bytes: "GITKA"
pub const ARCHIVE_MAGIC: &[u8; 5] = b"GITKA";

/// Current archive format version
pub const ARCHIVE_VERSION: u8 = 1;

/// Compression method byte values
pub const COMPRESSION_ZSTD: u8 = 0x01;
// Reserved for future SAI backend
// pub const COMPRESSION_SAI: u8 = 0x02;

/// Archive header structure (12 bytes)
#[repr(C, packed)]
pub struct ArchiveHeader {
    /// Magic bytes: "GITKA"
    pub magic: [u8; 5],
    /// Format version
    pub version: u8,
    /// Compression method byte (pluggable)
    pub compression_method: u8,
    /// Reserved for future use
    pub reserved: [u8; 5],
}

impl ArchiveHeader {
    /// Create a new header for zstd compression
    pub fn new_zstd() -> Self {
        Self {
            magic: *ARCHIVE_MAGIC,
            version: ARCHIVE_VERSION,
            compression_method: COMPRESSION_ZSTD,
            reserved: [0; 5],
        }
    }

    /// Create a new header for a specific compression method
    pub fn new(compression_method: u8) -> Self {
        Self {
            magic: *ARCHIVE_MAGIC,
            version: ARCHIVE_VERSION,
            compression_method,
            reserved: [0; 5],
        }
    }

    /// Serialize header to bytes
    pub fn to_bytes(&self) -> [u8; 12] {
        let mut bytes = [0u8; 12];
        bytes[0..5].copy_from_slice(&self.magic);
        bytes[5] = self.version;
        bytes[6] = self.compression_method;
        bytes[7..12].copy_from_slice(&self.reserved);
        bytes
    }

    /// Deserialize header from bytes
    pub fn from_bytes(bytes: &[u8; 12]) -> Result<Self> {
        let magic: [u8; 5] = bytes[0..5].try_into()
            .map_err(|_| GitkaError::Compression("Invalid header: magic bytes".to_string()))?;

        if &magic != ARCHIVE_MAGIC {
            return Err(GitkaError::Compression(
                "Invalid archive: bad magic bytes".to_string(),
            ));
        }

        Ok(Self {
            magic,
            version: bytes[5],
            compression_method: bytes[6],
            reserved: bytes[7..12].try_into()
                .map_err(|_| GitkaError::Compression("Invalid header: reserved bytes".to_string()))?,
        })
    }

    /// Validate the header
    pub fn validate(&self) -> Result<()> {
        if &self.magic != ARCHIVE_MAGIC {
            return Err(GitkaError::Compression(
                "Invalid archive: bad magic bytes".to_string(),
            ));
        }

        if self.version > ARCHIVE_VERSION {
            return Err(GitkaError::Compression(format!(
                "Unsupported archive version: {}",
                self.version
            )));
        }

        match self.compression_method {
            COMPRESSION_ZSTD => Ok(()),
            _ => Err(GitkaError::Compression(format!(
                "Unsupported compression method: 0x{:02X}",
                self.compression_method
            ))),
        }
    }

    /// Get the compression backend for this header
    pub fn compression_backend(&self) -> Result<CompressionBackend> {
        match self.compression_method {
            COMPRESSION_ZSTD => Ok(CompressionBackend::Zstd),
            _ => Err(GitkaError::Compression(format!(
                "Unsupported compression method: 0x{:02X}",
                self.compression_method
            ))),
        }
    }
}

/// Archive file wrapper
pub struct Archive {
    /// Path to the archive file
    pub path: PathBuf,
    /// Parsed header
    pub header: ArchiveHeader,
}

impl Archive {
    /// Open an existing archive
    pub fn open(path: &Path) -> Result<Self> {
        use std::fs::File;
        use std::io::Read;

        let mut file = File::open(path)
            .map_err(|e| GitkaError::Compression(format!("Failed to open archive: {}", e)))?;

        let mut header_bytes = [0u8; 12];
        file.read_exact(&mut header_bytes)
            .map_err(|e| GitkaError::Compression(format!("Failed to read header: {}", e)))?;

        let header = ArchiveHeader::from_bytes(&header_bytes)?;
        header.validate()?;

        Ok(Self {
            path: path.to_path_buf(),
            header,
        })
    }

    /// Create a new archive with zstd compression
    pub fn create_zstd(path: &Path) -> Result<Self> {
        use std::fs::File;
        use std::io::Write;

        let header = ArchiveHeader::new_zstd();
        let header_bytes = header.to_bytes();

        let mut file = File::create(path)
            .map_err(|e| GitkaError::Compression(format!("Failed to create archive: {}", e)))?;

        file.write_all(&header_bytes)
            .map_err(|e| GitkaError::Compression(format!("Failed to write header: {}", e)))?;

        Ok(Self {
            path: path.to_path_buf(),
            header,
        })
    }

    /// Get the file size
    pub fn size(&self) -> Result<u64> {
        std::fs::metadata(&self.path)
            .map(|m| m.len())
            .map_err(|e| GitkaError::Compression(format!("Failed to get archive size: {}", e)))
    }

    /// Check if this archive can be read by the current version
    pub fn is_compatible(&self) -> bool {
        self.header.version <= ARCHIVE_VERSION
    }
}

/// Get the compression method byte for a backend
pub fn compression_method_byte(backend: &CompressionBackend) -> u8 {
    match backend {
        CompressionBackend::Zstd => COMPRESSION_ZSTD,
    }
}

/// Get the archive extension for a compression method
pub fn archive_extension(method: u8) -> &'static str {
    match method {
        COMPRESSION_ZSTD => ".gitka.zst",
        _ => ".gitka",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_roundtrip() {
        let header = ArchiveHeader::new_zstd();
        let bytes = header.to_bytes();
        let parsed = ArchiveHeader::from_bytes(&bytes).unwrap();

        assert_eq!(header.magic, parsed.magic);
        assert_eq!(header.version, parsed.version);
        assert_eq!(header.compression_method, parsed.compression_method);
        assert_eq!(header.reserved, parsed.reserved);
    }

    #[test]
    fn test_header_validation() {
        let header = ArchiveHeader::new_zstd();
        assert!(header.validate().is_ok());

        let mut bad_header = header.clone();
        bad_header.magic = [0; 5];
        assert!(bad_header.validate().is_err());
    }
}

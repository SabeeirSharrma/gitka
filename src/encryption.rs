#![allow(dead_code)]

use std::path::Path;

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};

use crate::error::{GitkaError, Result};

/// Encryption key (32 bytes for AES-256)
pub type EncryptionKey = [u8; 32];

/// Encryption header prepended to encrypted files
#[derive(Debug, Clone)]
pub struct EncryptionHeader {
    /// Magic bytes: "GKENC" (5 bytes)
    pub magic: [u8; 5],
    /// Version (1 byte)
    pub version: u8,
    /// Nonce used for encryption (12 bytes)
    pub nonce: [u8; 12],
}

impl EncryptionHeader {
    pub const SIZE: usize = 5 + 1 + 12; // 18 bytes

    /// Create a new header with random nonce
    pub fn new() -> Self {
        let mut nonce = [0u8; 12];
        getrandom::fill(&mut nonce).expect("Failed to generate random nonce");

        Self {
            magic: *b"GKENC",
            version: 1,
            nonce,
        }
    }

    /// Serialize header to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::SIZE);
        bytes.extend_from_slice(&self.magic);
        bytes.push(self.version);
        bytes.extend_from_slice(&self.nonce);
        bytes
    }

    /// Parse header from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < Self::SIZE {
            return Err(GitkaError::Config("Header too short".to_string()));
        }

        let mut magic = [0u8; 5];
        magic.copy_from_slice(&bytes[..5]);

        if &magic != b"GKENC" {
            return Err(GitkaError::Config("Invalid encryption header magic".to_string()));
        }

        let version = bytes[5];
        if version != 1 {
            return Err(GitkaError::Config(format!("Unsupported encryption version: {}", version)));
        }

        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&bytes[6..18]);

        Ok(Self {
            magic,
            version,
            nonce,
        })
    }
}

/// Encrypt a file in-place
pub fn encrypt_file(file_path: &Path, key: &EncryptionKey) -> Result<u64> {
    use std::fs;
    use std::io::Write;

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| GitkaError::Config(format!("Failed to create cipher: {}", e)))?;

    // Read the original file
    let data = fs::read(file_path)
        .map_err(|e| GitkaError::Config(format!("Failed to read file: {}", e)))?;

    // Create header with random nonce
    let header = EncryptionHeader::new();
    let nonce = Nonce::try_from(header.nonce.as_slice())
        .map_err(|e| GitkaError::Config(format!("Failed to create nonce: {}", e)))?;

    // Encrypt the data
    let ciphertext = cipher.encrypt(&nonce, data.as_ref())
        .map_err(|e| GitkaError::Config(format!("Encryption failed: {}", e)))?;

    // Write header + ciphertext back to file
    let mut file = fs::File::create(file_path)
        .map_err(|e| GitkaError::Config(format!("Failed to create file: {}", e)))?;

    file.write_all(&header.to_bytes())
        .map_err(|e| GitkaError::Config(format!("Failed to write header: {}", e)))?;
    file.write_all(&ciphertext)
        .map_err(|e| GitkaError::Config(format!("Failed to write ciphertext: {}", e)))?;

    Ok((header.to_bytes().len() + ciphertext.len()) as u64)
}

/// Decrypt a file in-place
pub fn decrypt_file(file_path: &Path, key: &EncryptionKey) -> Result<u64> {
    use std::fs;
    use std::io::Write;

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| GitkaError::Config(format!("Failed to create cipher: {}", e)))?;

    // Read the file
    let data = fs::read(file_path)
        .map_err(|e| GitkaError::Config(format!("Failed to read file: {}", e)))?;

    // Parse header
    if data.len() < EncryptionHeader::SIZE {
        return Err(GitkaError::Config("File too small for encryption header".to_string()));
    }

    let header = EncryptionHeader::from_bytes(&data[..EncryptionHeader::SIZE])?;
    let nonce = Nonce::try_from(header.nonce.as_slice())
        .map_err(|e| GitkaError::Config(format!("Failed to create nonce: {}", e)))?;

    // Extract ciphertext
    let ciphertext = &data[EncryptionHeader::SIZE..];

    // Decrypt
    let plaintext = cipher.decrypt(&nonce, ciphertext)
        .map_err(|e| GitkaError::Config(format!("Decryption failed (wrong key?): {}", e)))?;

    // Write back decrypted data
    let mut file = fs::File::create(file_path)
        .map_err(|e| GitkaError::Config(format!("Failed to create file: {}", e)))?;

    file.write_all(&plaintext)
        .map_err(|e| GitkaError::Config(format!("Failed to write plaintext: {}", e)))?;

    Ok(plaintext.len() as u64)
}

/// Check if a file is encrypted
pub fn is_encrypted(file_path: &Path) -> Result<bool> {
    use std::fs::File;
    use std::io::Read;

    let mut file = File::open(file_path)
        .map_err(|e| GitkaError::Config(format!("Failed to open file: {}", e)))?;

    let mut header_bytes = [0u8; EncryptionHeader::SIZE];
    match file.read_exact(&mut header_bytes) {
        Ok(()) => {
            // Check magic bytes
            Ok(header_bytes[..5] == *b"GKENC")
        }
        Err(_) => Ok(false),
    }
}

/// Derive a key from a password using PBKDF2
pub fn derive_key(password: &str, salt: &[u8]) -> EncryptionKey {
    use sha2::{Sha256, Digest};

    let mut key = [0u8; 32];
    let mut hasher = Sha256::new();

    // Simple PBKDF2-like derivation (for production, use proper PBKDF2)
    for i in 0..10000 {
        hasher.update(password.as_bytes());
        hasher.update(salt);
        hasher.update(&(i as u32).to_le_bytes());
        let result = hasher.finalize();
        key.copy_from_slice(&result);
        hasher = Sha256::new();
    }

    key
}

/// Generate a random salt
pub fn generate_salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).expect("Failed to generate random salt");
    salt
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path();

        // Write test data
        let test_data = b"Hello, World! This is a test file.";
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(test_data).unwrap();
        drop(file);

        // Generate key
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).unwrap();

        // Encrypt
        encrypt_file(path, &key).unwrap();

        // Verify it's encrypted
        assert!(is_encrypted(path).unwrap());

        // Decrypt
        decrypt_file(path, &key).unwrap();

        // Verify decrypted data
        let decrypted = std::fs::read(path).unwrap();
        assert_eq!(decrypted, test_data);
    }
}

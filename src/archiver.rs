// Backend archiver logic - handles compression, encryption, and archive format

use anyhow::Result;
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const MAGIC_BYTES: &[u8; 4] = b"RPAK";
const VERSION: u8 = 1;
const NONCE_SIZE: usize = 12;

#[derive(Serialize, Deserialize, Debug)]
pub struct FileEntry {
    pub relative_path: String,
    pub original_size: u64,
    pub compressed_offset: u64,
    pub compressed_size: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ArchiveHeader {
    pub entries: Vec<FileEntry>,
    pub is_encrypted: bool,
    pub data_hash: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct PackProgress {
    pub current_file: String,
    pub processed: usize,
    pub total: usize,
}

pub struct Archiver;

impl Archiver {
    /// Pack files into a .rpak archive
    pub fn pack<F>(
        sources: &[PathBuf],
        output_path: &Path,
        password: Option<&str>,
        progress_callback: F,
    ) -> Result<()>
    where
        F: Fn(PackProgress),
    {
        // Collect all files to pack
        let mut files_to_pack = Vec::new();
        for source in sources {
            if source.is_file() {
                files_to_pack.push(source.clone());
            } else if source.is_dir() {
                for entry in WalkDir::new(source).into_iter().filter_map(|e| e.ok()) {
                    if entry.file_type().is_file() {
                        files_to_pack.push(entry.path().to_path_buf());
                    }
                }
            }
        }

        let total_files = files_to_pack.len();
        if total_files == 0 {
            anyhow::bail!("No files to pack");
        }

        // Compress all files and build entries
        let mut compressed_data = Vec::new();
        let mut entries = Vec::new();
        let base_path = Self::find_common_base(&files_to_pack);

        for (idx, file_path) in files_to_pack.iter().enumerate() {
            progress_callback(PackProgress {
                current_file: file_path.display().to_string(),
                processed: idx,
                total: total_files,
            });

            let relative_path = file_path
                .strip_prefix(&base_path)
                .unwrap_or(file_path)
                .to_string_lossy()
                .to_string();

            let mut file_data = Vec::new();
            File::open(file_path)?.read_to_end(&mut file_data)?;

            let original_size = file_data.len() as u64;
            let compressed = zstd::encode_all(&file_data[..], 3)?;
            let compressed_size = compressed.len() as u64;
            let compressed_offset = compressed_data.len() as u64;

            compressed_data.extend_from_slice(&compressed);

            entries.push(FileEntry {
                relative_path,
                original_size,
                compressed_offset,
                compressed_size,
            });
        }

        // Calculate hash of compressed data
        let data_hash = blake3::hash(&compressed_data).as_bytes().to_vec();

        // Encrypt if password provided
        let (final_data, is_encrypted) = if let Some(pwd) = password {
            let encrypted = Self::encrypt_data(&compressed_data, pwd)?;
            (encrypted, true)
        } else {
            (compressed_data, false)
        };

        // Build header
        let header = ArchiveHeader {
            entries,
            is_encrypted,
            data_hash,
        };

        // Write archive
        let mut output = File::create(output_path)?;
        output.write_all(MAGIC_BYTES)?;
        output.write_all(&[VERSION])?;

        let header_bytes = bincode::serialize(&header)?;
        let header_len = header_bytes.len() as u32;
        output.write_all(&header_len.to_le_bytes())?;
        output.write_all(&header_bytes)?;
        output.write_all(&final_data)?;

        progress_callback(PackProgress {
            current_file: "Complete".to_string(),
            processed: total_files,
            total: total_files,
        });

        Ok(())
    }

    /// Unpack a .rpak archive
    pub fn unpack<F>(
        archive_path: &Path,
        output_dir: &Path,
        password: Option<&str>,
        progress_callback: F,
    ) -> Result<()>
    where
        F: Fn(PackProgress),
    {
        let mut archive = File::open(archive_path)?;
        let mut buffer = Vec::new();
        archive.read_to_end(&mut buffer)?;

        // Verify magic bytes
        if &buffer[0..4] != MAGIC_BYTES {
            anyhow::bail!("Invalid archive format");
        }

        let version = buffer[4];
        if version != VERSION {
            anyhow::bail!("Unsupported archive version");
        }

        // Read header
        let header_len = u32::from_le_bytes([buffer[5], buffer[6], buffer[7], buffer[8]]) as usize;
        let header_bytes = &buffer[9..9 + header_len];
        let header: ArchiveHeader = bincode::deserialize(header_bytes)?;

        // Read compressed data
        let mut compressed_data = buffer[9 + header_len..].to_vec();

        // Decrypt if needed
        if header.is_encrypted {
            if password.is_none() {
                anyhow::bail!("Archive is encrypted but no password provided");
            }
            compressed_data = Self::decrypt_data(&compressed_data, password.unwrap())?;
        }

        // Verify integrity
        let calculated_hash = blake3::hash(&compressed_data).as_bytes().to_vec();
        if calculated_hash != header.data_hash {
            anyhow::bail!("Archive integrity check failed - file may be corrupted");
        }

        // Extract files
        fs::create_dir_all(output_dir)?;
        let total_files = header.entries.len();

        for (idx, entry) in header.entries.iter().enumerate() {
            progress_callback(PackProgress {
                current_file: entry.relative_path.clone(),
                processed: idx,
                total: total_files,
            });

            let start = entry.compressed_offset as usize;
            let end = start + entry.compressed_size as usize;
            let compressed_chunk = &compressed_data[start..end];

            let decompressed = zstd::decode_all(compressed_chunk)?;

            let output_path = output_dir.join(&entry.relative_path);
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let mut output_file = File::create(output_path)?;
            output_file.write_all(&decompressed)?;
        }

        progress_callback(PackProgress {
            current_file: "Complete".to_string(),
            processed: total_files,
            total: total_files,
        });

        Ok(())
    }

    fn encrypt_data(data: &[u8], password: &str) -> Result<Vec<u8>> {
        let key = Self::derive_key(password);
        let cipher = ChaCha20Poly1305::new(&key.into());
        let nonce = Nonce::from_slice(&[0u8; NONCE_SIZE]);

        let ciphertext = cipher
            .encrypt(nonce, data)
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

        Ok(ciphertext)
    }

    fn decrypt_data(data: &[u8], password: &str) -> Result<Vec<u8>> {
        let key = Self::derive_key(password);
        let cipher = ChaCha20Poly1305::new(&key.into());
        let nonce = Nonce::from_slice(&[0u8; NONCE_SIZE]);

        let plaintext = cipher
            .decrypt(nonce, data)
            .map_err(|_| anyhow::anyhow!("Decryption failed - wrong password?"))?;

        Ok(plaintext)
    }

    fn derive_key(password: &str) -> [u8; 32] {
        let hash = blake3::hash(password.as_bytes());
        *hash.as_bytes()
    }

    fn find_common_base(paths: &[PathBuf]) -> PathBuf {
        if paths.is_empty() {
            return PathBuf::new();
        }

        if paths.len() == 1 {
            return paths[0]
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
        }

        let first = &paths[0];
        let mut common = first.parent().unwrap_or(first).to_path_buf();

        for path in &paths[1..] {
            while !path.starts_with(&common) {
                if let Some(parent) = common.parent() {
                    common = parent.to_path_buf();
                } else {
                    return PathBuf::from(".");
                }
            }
        }

        common
    }
}

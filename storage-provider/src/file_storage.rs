use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use crate::storage::{ChunkStorage, Chunk};
use anyhow::Result;
use pod::FixedBytes;
use serde_json;
use async_trait::async_trait;
use sha2::{Sha256, Digest};

pub struct FileStorage {
    base_path: PathBuf,
}

impl FileStorage {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            base_path: path.as_ref().to_path_buf(),
        }
    }

    fn chunk_path(&self, namespace: String, commitment: FixedBytes<32>, index: u16) -> PathBuf {
        self.base_path.join(format!("{}_{}_{}.chunk", namespace, commitment, index))
    }

    fn ensure_dir_exists(&self) -> Result<()> {
        if !self.base_path.exists() {
            fs::create_dir_all(&self.base_path)?;
        }
        Ok(())
    }
}

#[async_trait]
impl ChunkStorage for FileStorage {
    async fn store(&self, namespace: String, commitment: FixedBytes<32>, chunk: &Chunk) -> Result<()> {
        self.ensure_dir_exists()?;

        // Store the chunk data
        let chunk_path = self.chunk_path(namespace, commitment, chunk.index);
        let mut file = File::create(&chunk_path)?;

        let serialized_chunk = serde_json::to_vec(&chunk)?;

        file.write_all(&serialized_chunk)?;

        Ok(())
    }

    async fn retrieve(&self, namespace: String, commitment: FixedBytes<32>, index: u16) -> Result<Option<Chunk>> {
        let chunk_path = self.chunk_path(namespace, commitment, index);

        if !chunk_path.exists() {
            return Ok(None);
        }

        // Read the chunk data
        let mut data = Vec::new();
        let mut file = File::open(&chunk_path)?;
        file.read_to_end(&mut data)?;

        let deserialized_chunk: Chunk = serde_json::from_slice(&data)?;

        Ok(Some(deserialized_chunk))
    }

    async fn exists(&self, namespace: String, commitment: FixedBytes<32>, index: u16) -> Result<bool> {
        let chunk_path = self.chunk_path(namespace, commitment, index);

        Ok(chunk_path.exists())
    }

    async fn delete(&self, namespace: String, commitment: FixedBytes<32>, index: u16) -> Result<bool> {
        let chunk_path = self.chunk_path(namespace, commitment, index);

        if !chunk_path.exists() {
            return Ok(false);
        }

        fs::remove_file(&chunk_path)?;
        Ok(true)
    }

    async fn list_chunks(&self, namespace: String, commitment: FixedBytes<32>) -> Result<Vec<u16>> {
        self.ensure_dir_exists()?;

        let mut chunks = Vec::new();
        for entry in fs::read_dir(&self.base_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("chunk") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    // Parse the filename to extract the index
                    // Filename format: {namespace}_{commitment}_{index}.chunk
                    let parts: Vec<&str> = stem.split('_').collect();
                    if parts.len() >= 3 {
                        if let Ok(index) = parts[2].parse::<u16>() {
                            chunks.push(index);
                        }
                    }
                }
            }
        }

        // Sort chunks for consistent ordering
        chunks.sort();
        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::FixedBytes;
    use tempfile::TempDir;

    fn hash(data: &[u8]) -> FixedBytes<32> {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash = hasher.finalize();
        FixedBytes::from_slice(&hash)
    }

    async fn setup() -> (FileStorage, TempDir, String, FixedBytes<32>) {
        let namespace = "test-namespace".to_string();
        let commitment = hash(b"full-data");
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::new(temp_dir.path());

        (storage, temp_dir, namespace, commitment)
    }

    fn create_test_chunk(index: u16) -> Chunk {
        Chunk {
            index,
            data: b"Hello, World!".to_vec(),
            hash: hash(b"chunk-data"),
            merkle_proof: vec!["proof1".to_string(), "proof2".to_string()],
        }
    }


    #[tokio::test]
    async fn test_store_and_retrieve() {
        let (storage, _temp_dir, namespace, commitment) = setup().await;
        let chunk = create_test_chunk(1);

        // Test store
        storage.store(namespace.clone(), commitment, &chunk).await.unwrap();

        // Test retrieve
        let retrieved = storage.retrieve(namespace, commitment, 1).await.unwrap().unwrap();
        assert_eq!(retrieved.data, chunk.data);
        assert_eq!(retrieved.index, chunk.index);
        assert_eq!(retrieved.hash, chunk.hash);
        assert_eq!(retrieved.merkle_proof, chunk.merkle_proof);
    }

    #[tokio::test]
    async fn test_exists() {
        let (storage, _temp_dir, namespace, commitment) = setup().await;
        let chunk = create_test_chunk(1);

        // Initially should not exist
        assert!(!storage.exists(namespace.clone(), commitment, 1).await.unwrap());

        // Store the chunk
        storage.store(namespace.clone(), commitment, &chunk).await.unwrap();

        // Should exist after storing
        assert!(storage.exists(namespace, commitment, 1).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete() {
        let (storage, _temp_dir, namespace, commitment) = setup().await;
        let chunk = create_test_chunk(1);

        // Store the chunk
        storage.store(namespace.clone(), commitment, &chunk).await.unwrap();
        assert!(storage.exists(namespace.clone(), commitment, 1).await.unwrap());

        // Delete the chunk
        assert!(storage.delete(namespace.clone(), commitment, 1).await.unwrap());
        assert!(!storage.exists(namespace.clone(), commitment, 1).await.unwrap());

        // Delete non-existent chunk should return false
        assert!(!storage.delete(namespace, commitment, 999).await.unwrap());
    }

    #[tokio::test]
    async fn test_list_chunks() {
        let (storage, _temp_dir, namespace, commitment) = setup().await;

        // Store multiple chunks
        for i in 1..=5 {
            let chunk = create_test_chunk(i);
            storage.store(namespace.clone(), commitment, &chunk).await.unwrap();
        }

        // Test listing chunks
        let listed = storage.list_chunks(namespace, commitment).await.unwrap();
        assert_eq!(listed.len(), 5);
        assert_eq!(listed, vec![1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn test_retrieve_nonexistent() {
        let (storage, _temp_dir, namespace, commitment) = setup().await;
        let result = storage.retrieve(namespace, commitment, 999).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_corrupted_chunk() {
        let (storage, _temp_dir, namespace, commitment) = setup().await;
        let chunk = create_test_chunk(1);

        // Store valid data
        storage.store(namespace.clone(), commitment, &chunk).await.unwrap();

        // Corrupt the chunk file by writing invalid JSON
        let chunk_path = storage.chunk_path(namespace.clone(), commitment, 1);
        std::fs::write(chunk_path, "invalid json").unwrap();

        // Attempt to retrieve should fail
        assert!(storage.retrieve(namespace, commitment, 1).await.is_err());
    }
}
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use anyhow::Result;
use merkle_tree::MerkleProof;
use pod::FixedBytes;
use serde::{Deserialize, Serialize};
use serde_json;
use async_trait::async_trait;
use common::types::Chunk;
use crate::storage::ChunkStorageTrait;

pub struct FileStorage {
    base_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkWithProof {
    pub chunk: Chunk,
    pub merkle_proof: MerkleProof,
}

impl FileStorage {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            base_path: path.as_ref().to_path_buf(),
        }
    }

    fn chunk_path(&self, commitment: FixedBytes<32>, index: u16) -> PathBuf {
        self.base_path.join(format!("{}_{}.chunk", commitment, index))
    }

    fn ensure_dir_exists(&self) -> Result<()> {
        if !self.base_path.exists() {
            fs::create_dir_all(&self.base_path)?;
        }
        Ok(())
    }
}

#[async_trait]
impl ChunkStorageTrait for FileStorage {
    async fn store(&self, commitment: FixedBytes<32>, chunk: &Chunk, merkle_proof: &MerkleProof) -> Result<()> {
        self.ensure_dir_exists()?;

        // Store the chunk data
        let chunk_path = self.chunk_path(commitment, chunk.index);
        let mut file = File::create(&chunk_path)?;

        let chunk_with_proof = ChunkWithProof { chunk: chunk.clone(), merkle_proof: merkle_proof.clone() };
        let serialized_chunk = serde_json::to_vec(&chunk_with_proof)?;

        file.write_all(&serialized_chunk)?;

        Ok(())
    }

    async fn retrieve(&self, commitment: FixedBytes<32>, index: u16) -> Result<Option<(Chunk, MerkleProof)>> {
        let chunk_path = self.chunk_path(commitment, index);

        if !chunk_path.exists() {
            return Ok(None);
        }

        // Read the chunk data
        let mut data = Vec::new();
        let mut file = File::open(&chunk_path)?;
        file.read_to_end(&mut data)?;

        let deserialized_chunk: ChunkWithProof = serde_json::from_slice(&data)?;
        if deserialized_chunk.chunk.index != index {
            return Err(anyhow::anyhow!("Chunk index mismatch"));
        }

        Ok(Some((deserialized_chunk.chunk.clone(), deserialized_chunk.merkle_proof.clone())))
    }

    async fn exists(&self, commitment: FixedBytes<32>, index: u16) -> Result<bool> {
        let chunk_path = self.chunk_path(commitment, index);

        Ok(chunk_path.exists())
    }

    async fn delete(&self, commitment: FixedBytes<32>, index: u16) -> Result<bool> {
        let chunk_path = self.chunk_path(commitment, index);

        if !chunk_path.exists() {
            return Ok(false);
        }

        fs::remove_file(&chunk_path)?;
        Ok(true)
    }

    async fn list_chunks(&self, commitment: FixedBytes<32>) -> Result<Vec<u16>> {
        self.ensure_dir_exists()?;

        let mut chunks = Vec::new();
        for entry in fs::read_dir(&self.base_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("chunk") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    // Filename format: {commitment}_{index}.chunk
                    let parts: Vec<&str> = stem.split('_').collect();
                    if parts.len() >= 2 {
                        if let Ok(index) = parts[1].parse::<u16>() {
                            if parts[0] == commitment.to_string() {
                                chunks.push(index);
                            }
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
    use pod::FixedBytes;
    use sha3::{Digest, Keccak256};
    use tempfile::TempDir;

    async fn setup() -> (FileStorage, TempDir, FixedBytes<32>) {
        let commitment = FixedBytes::from_slice(&Keccak256::digest(b"full-data"));
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::new(temp_dir.path());

        (storage, temp_dir, commitment)
    }

    fn create_test_chunk(index: u16) -> Chunk {
        Chunk {
            index,
            data: b"Hello, World!".to_vec(),
        }
    }


    #[tokio::test]
    async fn test_store_and_retrieve() {
        let (storage, _temp_dir, commitment) = setup().await;
        let chunk = create_test_chunk(1);
        let merkle_proof = MerkleProof {
            path: vec![],
        };

        // Test store
        storage.store(commitment, &chunk, &merkle_proof).await.unwrap();

        // Test retrieve
        let (retrieved_chunk, _) = storage.retrieve(commitment, 1).await.unwrap().unwrap();
        assert_eq!(retrieved_chunk.data, chunk.data);
        assert_eq!(retrieved_chunk.index, chunk.index);
        assert_eq!(retrieved_chunk.hash(), chunk.hash());
    }

    #[tokio::test]
    async fn test_exists() {
        let (storage, _temp_dir, commitment) = setup().await;
        let chunk = create_test_chunk(1);
        let merkle_proof = MerkleProof {
            path: vec![],
        };

        // Initially should not exist
        assert!(!storage.exists(commitment, 1).await.unwrap());

        // Store the chunk
        storage.store(commitment, &chunk, &merkle_proof).await.unwrap();

        // Should exist after storing
        assert!(storage.exists(commitment, 1).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete() {
        let (storage, _temp_dir, commitment) = setup().await;
        let chunk = create_test_chunk(1);
        let merkle_proof = MerkleProof {
            path: vec![],
        };

        // Store the chunk
        storage.store(commitment, &chunk, &merkle_proof).await.unwrap();
        assert!(storage.exists(commitment, 1).await.unwrap());

        // Delete the chunk
        assert!(storage.delete(commitment, 1).await.unwrap());
        assert!(!storage.exists(commitment, 1).await.unwrap());

        // Delete non-existent chunk should return false
        assert!(!storage.delete(commitment, 999).await.unwrap());
    }

    #[tokio::test]
    async fn test_list_chunks() {
        let (storage, _temp_dir, commitment) = setup().await;
        let merkle_proof = MerkleProof {
            path: vec![],
        };

        // Store multiple chunks
        for i in 1..=5 {
            let chunk = create_test_chunk(i);
            storage.store(commitment, &chunk, &merkle_proof).await.unwrap();
        }

        // Test listing chunks
        let listed = storage.list_chunks(commitment).await.unwrap();
        assert_eq!(listed.len(), 5);
        assert_eq!(listed, vec![1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn test_retrieve_nonexistent() {
        let (storage, _temp_dir, commitment) = setup().await;
        let result = storage.retrieve(commitment, 999).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_corrupted_chunk() {
        let (storage, _temp_dir, commitment) = setup().await;
        let chunk = create_test_chunk(1);
        let merkle_proof = MerkleProof {
            path: vec![],
        };

        // Store valid data
        storage.store(commitment, &chunk, &merkle_proof).await.unwrap();

        // Corrupt the chunk file by writing invalid JSON
        let chunk_path = storage.chunk_path(commitment, 1);
        std::fs::write(chunk_path, "invalid json").unwrap();

        // Attempt to retrieve should fail
        assert!(storage.retrieve(commitment, 1).await.is_err());
    }
}
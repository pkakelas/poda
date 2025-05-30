use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use crate::storage::{ChunkStorage, ChunkMetadata, ChunkData};
use anyhow::Result;
use serde_json;
use async_trait::async_trait;

pub struct FileStorage {
    base_path: PathBuf,
}

impl FileStorage {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            base_path: path.as_ref().to_path_buf(),
        }
    }

    fn chunk_path(&self, chunk_id: &str) -> PathBuf {
        self.base_path.join(format!("{}.chunk", chunk_id))
    }

    fn metadata_path(&self, chunk_id: &str) -> PathBuf {
        self.base_path.join(format!("{}.meta", chunk_id))
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
    async fn store(&self, chunk_id: &str, data: &[u8], metadata: ChunkMetadata) -> Result<()> {
        self.ensure_dir_exists()?;

        // Store the chunk data
        let chunk_path = self.chunk_path(chunk_id);
        let mut file = File::create(&chunk_path)?;
        file.write_all(data)?;

        // Store the metadata
        let metadata_path = self.metadata_path(chunk_id);
        let metadata_file = File::create(&metadata_path)?;
        serde_json::to_writer(metadata_file, &metadata)?;

        Ok(())
    }

    async fn retrieve(&self, chunk_id: &str) -> Result<Option<ChunkData>> {
        let chunk_path = self.chunk_path(chunk_id);
        let metadata_path = self.metadata_path(chunk_id);

        // Check if both files exist
        if !chunk_path.exists() || !metadata_path.exists() {
            return Ok(None);
        }

        // Read the chunk data
        let mut data = Vec::new();
        let mut file = File::open(&chunk_path)?;
        file.read_to_end(&mut data)?;

        // Read the metadata
        let metadata_file = File::open(&metadata_path)?;
        let metadata: ChunkMetadata = serde_json::from_reader(metadata_file)?;

        Ok(Some(ChunkData { data, metadata }))
    }

    async fn exists(&self, chunk_id: &str) -> Result<bool> {
        let chunk_path = self.chunk_path(chunk_id);
        let metadata_path = self.metadata_path(chunk_id);
        Ok(chunk_path.exists() && metadata_path.exists())
    }

    async fn delete(&self, chunk_id: &str) -> Result<bool> {
        let chunk_path = self.chunk_path(chunk_id);
        let metadata_path = self.metadata_path(chunk_id);

        let mut deleted = false;
        if chunk_path.exists() {
            fs::remove_file(&chunk_path)?;
            deleted = true;
        }
        if metadata_path.exists() {
            fs::remove_file(&metadata_path)?;
            deleted = true;
        }

        Ok(deleted)
    }

    async fn list_chunks(&self, offset: usize, limit: usize) -> Result<Vec<String>> {
        self.ensure_dir_exists()?;
        
        let mut chunks = Vec::new();
        for entry in fs::read_dir(&self.base_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("chunk") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    chunks.push(stem.to_string());
                }
            }
        }

        // Sort chunks for consistent pagination
        chunks.sort();
        
        // Apply pagination
        let start = offset.min(chunks.len());
        let end = (offset + limit).min(chunks.len());
        Ok(chunks[start..end].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::time::SystemTime;

    async fn setup() -> (FileStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::new(temp_dir.path());
        (storage, temp_dir)
    }

    fn create_test_metadata() -> ChunkMetadata {
        ChunkMetadata {
            namespace: "namespace1".to_string(),
            index: 1,
            hash: "test-hash".to_string(),
            stored_at: SystemTime::now(),
        }
    }

    #[tokio::test]
    async fn test_store_and_retrieve() {
        let (storage, _temp_dir) = setup().await;
        let chunk_id = "test-chunk-1";
        let data = b"Hello, World!";
        let metadata = create_test_metadata();

        // Test store
        storage.store(chunk_id, data, metadata.clone()).await.unwrap();

        // Test retrieve
        let retrieved = storage.retrieve(chunk_id).await.unwrap().unwrap();
        assert_eq!(retrieved.data, data);
        assert_eq!(retrieved.metadata.namespace, metadata.namespace);
        assert_eq!(retrieved.metadata.index, metadata.index);
        assert_eq!(retrieved.metadata.hash, metadata.hash);
    }

    #[tokio::test]
    async fn test_exists() {
        let (storage, _temp_dir) = setup().await;
        let chunk_id = "test-chunk-2";
        let data = b"Test data";
        let metadata = create_test_metadata();

        // Initially should not exist
        assert!(!storage.exists(chunk_id).await.unwrap());

        // Store the chunk
        storage.store(chunk_id, data, metadata).await.unwrap();

        // Should exist after storing
        assert!(storage.exists(chunk_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete() {
        let (storage, _temp_dir) = setup().await;
        let chunk_id = "test-chunk-3";
        let data = b"Test data";
        let metadata = create_test_metadata();

        // Store the chunk
        storage.store(chunk_id, data, metadata).await.unwrap();
        assert!(storage.exists(chunk_id).await.unwrap());

        // Delete the chunk
        assert!(storage.delete(chunk_id).await.unwrap());
        assert!(!storage.exists(chunk_id).await.unwrap());

        // Delete non-existent chunk should return false
        assert!(!storage.delete("non-existent").await.unwrap());
    }

    #[tokio::test]
    async fn test_list_chunks() {
        let (storage, _temp_dir) = setup().await;
        let metadata = create_test_metadata();

        // Store multiple chunks
        let chunks = vec!["chunk-1", "chunk-2", "chunk-3", "chunk-4", "chunk-5"];
        for chunk_id in &chunks {
            storage.store(chunk_id, b"data", metadata.clone()).await.unwrap();
        }

        // Test pagination
        let listed = storage.list_chunks(0, 2).await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0], "chunk-1");
        assert_eq!(listed[1], "chunk-2");

        // Test offset
        let listed = storage.list_chunks(2, 2).await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0], "chunk-3");
        assert_eq!(listed[1], "chunk-4");

        // Test limit larger than available
        let listed = storage.list_chunks(4, 10).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0], "chunk-5");
    }

    #[tokio::test]
    async fn test_retrieve_nonexistent() {
        let (storage, _temp_dir) = setup().await;
        let result = storage.retrieve("non-existent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_corrupted_metadata() {
        let (storage, _temp_dir) = setup().await;
        let chunk_id = "corrupted-chunk";
        let data = b"Test data";
        let metadata = create_test_metadata();

        // Store valid data
        storage.store(chunk_id, data, metadata).await.unwrap();

        // Corrupt the metadata file
        let metadata_path = storage.metadata_path(chunk_id);
        std::fs::write(metadata_path, "invalid json").unwrap();

        // Attempt to retrieve should fail
        assert!(storage.retrieve(chunk_id).await.is_err());
    }
}
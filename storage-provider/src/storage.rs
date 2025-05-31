use anyhow::Result;
use std::time::SystemTime;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub data: Vec<u8>,
    pub metadata: ChunkMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub namespace: String,
    pub index: u32,
    pub hash: String,
    pub stored_at: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredChunk {    
    pub data: Vec<u8>,
    pub metadata: ChunkMetadata,
}

#[async_trait::async_trait]
pub trait ChunkStorage {
    async fn store(&self, chunk_id: &str, data: &[u8], metadata: ChunkMetadata) -> Result<()>;
    async fn retrieve(&self, chunk_id: &str) -> Result<Option<Chunk>>;
    async fn exists(&self, chunk_id: &str) -> Result<bool>;
    async fn delete(&self, chunk_id: &str) -> Result<bool>;
    async fn list_chunks(&self, offset: usize, limit: usize) -> Result<Vec<String>>;
}
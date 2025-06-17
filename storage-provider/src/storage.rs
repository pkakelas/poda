use alloy::primitives::FixedBytes;
use anyhow::Result;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub index: u16,
    pub data: Vec<u8>,
    pub hash: FixedBytes<32>,
    pub merkle_proof: Vec<String>,
}

#[async_trait::async_trait]
pub trait ChunkStorage {
    async fn store(&self, namespace: String, commitment: FixedBytes<32>, chunk: &Chunk) -> Result<()>;
    async fn retrieve(&self, namespace: String, commitment: FixedBytes<32>, index: u16) -> Result<Option<Chunk>>;
    async fn exists(&self, namespace: String, commitment: FixedBytes<32>, index: u16) -> Result<bool>;
    async fn delete(&self, namespace: String, commitment: FixedBytes<32>, index: u16) -> Result<bool>;
    async fn list_chunks(&self, namespace: String, commitment: FixedBytes<32>) -> Result<Vec<u16>>;
}
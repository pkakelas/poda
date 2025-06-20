use alloy::primitives::FixedBytes;
use anyhow::Result;
use merkle_tree::MerkleProof;
use types::Chunk;

#[async_trait::async_trait]
pub trait ChunkStorage {
    async fn store(&self, namespace: String, commitment: FixedBytes<32>, chunk: &Chunk, merkle_proof: &MerkleProof) -> Result<()>;
    async fn retrieve(&self, namespace: String, commitment: FixedBytes<32>, index: u16) -> Result<Option<(Chunk, MerkleProof)>>;
    async fn exists(&self, namespace: String, commitment: FixedBytes<32>, index: u16) -> Result<bool>;
    async fn delete(&self, namespace: String, commitment: FixedBytes<32>, index: u16) -> Result<bool>;
    async fn list_chunks(&self, namespace: String, commitment: FixedBytes<32>) -> Result<Vec<u16>>;
}
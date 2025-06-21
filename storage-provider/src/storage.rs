use alloy::primitives::FixedBytes;
use anyhow::Result;
use merkle_tree::MerkleProof;
use types::Chunk;

#[async_trait::async_trait]
pub trait ChunkStorageTrait {
    async fn store(&self, commitment: FixedBytes<32>, chunk: &Chunk, merkle_proof: &MerkleProof) -> Result<()>;
    async fn retrieve(&self, commitment: FixedBytes<32>, index: u16) -> Result<Option<(Chunk, MerkleProof)>>;
    async fn exists(&self, commitment: FixedBytes<32>, index: u16) -> Result<bool>;
    async fn delete(&self, commitment: FixedBytes<32>, index: u16) -> Result<bool>;
    async fn list_chunks(&self, commitment: FixedBytes<32>) -> Result<Vec<u16>>;
}
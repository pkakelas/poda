mod hash;  
mod tree;

use types::Chunk;
use anyhow::Result;
pub use crate::tree::{MerkleProof, MerkleTree, StandardMerkleTree};

pub fn gen_merkle_tree(chunks: &[Chunk]) -> StandardMerkleTree {
    let leaves = chunks.iter().map(|chunk| chunk.hash()).collect::<Vec<_>>();
    StandardMerkleTree::new(leaves)
}

pub fn gen_proof(merkle_tree: &StandardMerkleTree, leaf: Chunk) -> Result<MerkleProof> {
    merkle_tree.generate_proof(leaf.hash())
}

pub fn verify_proof(merkle_tree: &StandardMerkleTree, leaf: Chunk, proof: MerkleProof) -> bool {
    MerkleTree::verify_proof(merkle_tree.root(), leaf.hash(), proof)
}

#[cfg(test)]
mod tests {
    use types::{keccak256, SolValue};

    use super::*;

    fn get_sample_chunks() -> Vec<Chunk> {
        vec![
            Chunk { index: 0, data: b"hello".to_vec() },
            Chunk { index: 1, data: b"world".to_vec() },
            Chunk { index: 2, data: b"hello".to_vec() },
            Chunk { index: 3, data: b"world".to_vec() }
        ]
    }

    #[test]
    fn test_chunk_hash() {
        let data = "hello".as_bytes();
        let chunk = Chunk { index: 1, data: data.to_vec().clone() };
        let hash = chunk.hash();
        assert_eq!(hash, keccak256((1, keccak256(data)).abi_encode()));
    }

    #[test]
    fn test_merkle_tree_proof() {
        let chunks = get_sample_chunks();
        let merkle_tree = gen_merkle_tree(&chunks);

        for chunk in chunks {
            let proof = gen_proof(&merkle_tree, chunk.clone()).unwrap();
            assert_eq!(verify_proof(&merkle_tree, chunk.clone(), proof), true);
        }
    }

    #[test]
    fn test_merkle_tree_invalid_proof() {
        let chunks = get_sample_chunks();
        let merkle_tree = gen_merkle_tree(&chunks);

        let proof = gen_proof(&merkle_tree, chunks[0].clone()).unwrap();
        assert_eq!(verify_proof(&merkle_tree, chunks[1].clone(), proof), false);
    }
}
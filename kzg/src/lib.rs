mod kzg;
mod utils;
pub mod types;

use ark_bls12_381::{Bls12_381, Fr, FrConfig, G1Projective as G1, G2Projective as G2};
use ark_ec::PrimeGroup;
use ark_ff::AdditiveGroup;
use ark_ff::{Fp, MontBackend};
use ::types::{Chunk, constants::TOTAL_SHARDS};
use types::{KzgCommitment, KzgProof};
use kzg::KZG;
use utils::interpolate;
use utils::{load_ethereum_ceremony};
use std::sync::OnceLock;
use std::sync::Arc;

pub type KZGPolynomial = Vec<ark_ff::Fp<ark_ff::MontBackend<ark_bls12_381::FrConfig, 4>, 4>>;

static KZG_INSTANCE: OnceLock<Arc<KZG<Bls12_381>>> = OnceLock::new();
static PATH: &str = "../kzg/ethereum_ceremony.json";

fn get_kzg_instance() -> Arc<KZG<Bls12_381>> {
    KZG_INSTANCE.get_or_init(|| {
        // Try to find the ethereum_ceremony.json file
        // Try to load from Ethereum ceremony file first
        let kzg = match load_ethereum_ceremony(PATH, TOTAL_SHARDS - 1) {
            Ok((crs_g1, crs_g2)) => {
                // Use Ethereum's trusted setup ceremony data
                let g1 = G1::generator();
                let g2 = G2::generator();
                KZG::<Bls12_381>::from_trusted_setup(g1, g2, TOTAL_SHARDS - 1, crs_g1, crs_g2)
                    .expect("Failed to create KZG from Ethereum ceremony data")
            },
            Err(e) => {
                panic!("Failed to load Ethereum ceremony data from {}: {}", PATH, e);
            }
        };
        Arc::new(kzg)
    }).clone()
}

pub fn kzg_commit(chunks: &Vec<Chunk>) -> (KzgCommitment, KZGPolynomial) {
    // Convert all chunks to field elements (one field element per chunk)
    let mut all_field_elements = Vec::new();
    
    for chunk in chunks {
        let field_elements = chunk_to_field_elements(chunk);
        all_field_elements.extend(field_elements);
    }

    let polynomial = gen_polynomial(chunks, get_kzg_instance().degree);
    let commitment = get_kzg_instance().commit(&polynomial);

    return (KzgCommitment::new(commitment), polynomial);
}

pub fn kzg_prove(chunks: &Vec<Chunk>, chunk_index: usize) -> KzgProof {
    let (_, polynomial) = kzg_commit(chunks);

    let proof_point = Fr::from(chunk_index as u64);
    let proof = get_kzg_instance().open(&polynomial, proof_point);

    KzgProof::new(proof)
}

pub fn kzg_multi_prove(chunks: &Vec<Chunk>, chunk_indices: &[usize]) -> KzgProof {
    let (_, polynomial) = kzg_commit(chunks);

    let points: Vec<Fr> = chunk_indices.iter().map(|i| Fr::from(*i as u64)).collect();
    let proof = get_kzg_instance().multi_open(&polynomial, &points);

    KzgProof::new(proof)
}

pub fn kzg_multi_verify(chunks: &[Chunk], chunk_indices: &[usize], commitment: KzgCommitment, proof: KzgProof) -> bool {
    let points: Vec<Fr> = chunk_indices.iter().map(|i| Fr::from(*i as u64)).collect();
    let values: Vec<Fp<MontBackend<FrConfig, 4>, 4>> = chunks.iter().map(chunk_to_field_elements).map(|v| v[0]).collect();

    get_kzg_instance().verify_multi(&points, &values, commitment.into_inner(), proof.into_inner())
}

pub fn kzg_verify(chunk: &Chunk, chunk_index: usize, commitment: KzgCommitment, proof: KzgProof) -> bool {
    // Convert the chunk to field elements
    let field_elements = chunk_to_field_elements(chunk);
    let fr_value = field_elements[0];
    
    // Use the same KZG instance that was used for commitment and proof generation
    let point = Fr::from(chunk_index as u64);
    get_kzg_instance().verify(point, fr_value, commitment.into_inner(), proof.into_inner())
}

/// Convert a chunk to field elements for interpolation
fn chunk_to_field_elements(chunk: &Chunk) -> Vec<Fr> {
    // Use the first 4 bytes of the hash to create exactly one field element per chunk
    let mut combined: u32 = 0;
    for (i, &byte) in chunk.hash.as_slice().iter().take(4).enumerate() {
        combined |= (byte as u32) << (8 * i);
    }
    vec![Fr::from(combined)]
}

fn gen_polynomial(data: &Vec<Chunk>, degree: usize) -> Vec<Fp<MontBackend<FrConfig, 4>, 4>> {
    // Convert all chunks to field elements to reconstruct the polynomial
    let mut all_field_elements = Vec::new();
    
    for chunk in data {
        let field_elements = chunk_to_field_elements(chunk);
        all_field_elements.extend(field_elements);
    }

    // Create interpolation points [0, 1, 2, ..., n-1]
    let points: Vec<Fr> = (0..all_field_elements.len())
        .map(|i| Fr::from(i as u64))
        .collect();

    // Interpolate to get the polynomial (same as in commit)
    let mut interpolated_poly = interpolate(&points, &all_field_elements).unwrap();
    
    // Ensure polynomial fits within KZG degree
    if interpolated_poly.len() > degree + 1 {
        interpolated_poly.truncate(degree + 1);
    } else if interpolated_poly.len() < degree + 1 {
        interpolated_poly.resize(degree + 1, Fr::ZERO);
    }

    interpolated_poly
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod::FixedBytes;
    use rand::random;
    use sha3::{Digest, Keccak256};

    fn get_sample_chunks() -> Vec<Chunk> {
        let mut chunks = Vec::new();
        for i in 0..18 {
            let mut data = vec![];

            for _ in 0..120 {
                let random_byte: u8 = random();
                data.push(random_byte);
            }

            chunks.push(Chunk {
                index: i,
                data: data.clone(),
                hash: FixedBytes::from_slice(&Keccak256::digest(&data)),
            });
        }

        chunks
    }

    #[test]
    fn test_kzg_manager_basic_functionality() {
        let chunks = get_sample_chunks();

        // Generate commitment
        let (commitment, _) = kzg_commit(&chunks);

        // Generate proof for first chunk
        let proof = kzg_prove(&chunks, 0);

        // Verify the proof
        let is_valid = kzg_verify(&chunks[0], 0, commitment.clone(), proof);
        assert!(is_valid, "Proof verification should succeed");

        let invalid_proof = kzg_prove(&chunks, 1);
        let is_invalid = kzg_verify(&chunks[0], 1, commitment.clone(), invalid_proof);
        assert!(!is_invalid, "Proof verification should fail if chunk index is not correct");

        let invalid_proof = kzg_prove(&chunks, 0);
        let is_invalid = kzg_verify(&chunks[1], 0, commitment, invalid_proof);
        assert!(!is_invalid, "Proof verification should fail if chunk is not correct");
    }

    #[test]
    fn test_kzg_manager_multi_prove_and_verify() {
        let chunks = get_sample_chunks();
        let (commitment, _) = kzg_commit(&chunks);

        let selected_chunks = chunks[..2].to_vec();
        let proof = kzg_multi_prove(&chunks, &[0, 1]);
        let is_valid = kzg_multi_verify(&selected_chunks, &[0, 1], commitment.clone(), proof);
        assert!(is_valid, "Multi-prove and verify should succeed");

        // Test with wrong chunks for the same indices
        let wrong_chunks = vec![chunks[4].clone(), chunks[5].clone()];
        let invalid_proof = kzg_multi_prove(&chunks, &[1, 2]);
        let is_valid = kzg_multi_verify(&wrong_chunks, &[1, 2], commitment.clone(), invalid_proof);
        assert!(!is_valid, "Multi-prove and verify should fail if chunk is not correct");

        // Test with wrong indices for the same chunks
        let selected_chunks = chunks[..2].to_vec();
        let invalid_proof = kzg_multi_prove(&chunks, &[0, 1]);
        let is_valid = kzg_multi_verify(&selected_chunks, &[0, 5], commitment.clone(), invalid_proof);
        assert!(!is_valid, "Multi-prove and verify should fail if chunk index is not correct");

        // Test with wrong chunks and indices
        let wrong_chunks = vec![chunks[1].clone(), chunks[2].clone()];
        let invalid_proof = kzg_multi_prove(&chunks, &[0, 1]);
        let is_valid = kzg_multi_verify(&wrong_chunks, &[0, 1], commitment, invalid_proof);
        assert!(!is_valid, "Multi-prove and verify should fail if chunk index is not correct");
    }

    #[test]
    fn test_kzg_manager_all_chunks_prove_and_verify() {
        let chunks = get_sample_chunks();
        let (commitment, _) = kzg_commit(&chunks);

        // Test single proof for each chunk from 0 to 17
        for i in 0..18 {
            let proof = kzg_prove(&chunks, i);
            let is_valid = kzg_verify(&chunks[i], i, commitment.clone(), proof);
            assert!(is_valid, "Single proof verification should succeed for chunk {}", i);
        }

        // Test multi-proof for all chunks
        let all_indices: Vec<usize> = (0..18).collect();
        let proof = kzg_multi_prove(&chunks, &all_indices);
        let is_valid = kzg_multi_verify(&chunks, &all_indices, commitment.clone(), proof);
        assert!(is_valid, "Multi-proof verification should succeed for all chunks");

        // Test multi-proof for a subset of chunks
        let subset_indices = vec![0, 5, 10, 15];
        let subset_chunks = subset_indices.iter().map(|&i| chunks[i].clone()).collect::<Vec<_>>();
        let proof = kzg_multi_prove(&chunks, &subset_indices);
        let is_valid = kzg_multi_verify(&subset_chunks, &subset_indices, commitment, proof);
        assert!(is_valid, "Multi-proof verification should succeed for subset of chunks");
    }

    #[test]
    fn test_kzg_manager_with_total_shards() {
        use ::types::constants::TOTAL_SHARDS;
        
        // Create chunks for the actual TOTAL_SHARDS
        let mut chunks = Vec::new();
        for i in 0..TOTAL_SHARDS {
            let mut data = vec![];
            for _ in 0..120 {
                let random_byte: u8 = random();
                data.push(random_byte);
            }

            chunks.push(Chunk {
                index: i as u16,
                data: data.clone(),
                hash: FixedBytes::from_slice(&Keccak256::digest(&data)),
            });
        }

        let (commitment, _) = kzg_commit(&chunks);

        // Test single proof for each chunk from 0 to TOTAL_SHARDS-1
        for i in 0..TOTAL_SHARDS {
            let proof = kzg_prove(&chunks, i);
            let is_valid = kzg_verify(&chunks[i], i, commitment.clone(), proof);
            assert!(is_valid, "Single proof verification should succeed for chunk {}", i);
        }

        // Test multi-proof for all chunks
        let all_indices: Vec<usize> = (0..TOTAL_SHARDS).collect();
        let proof = kzg_multi_prove(&chunks, &all_indices);
        let is_valid = kzg_multi_verify(&chunks, &all_indices, commitment.clone(), proof);
        assert!(is_valid, "Multi-proof verification should succeed for all {} chunks", TOTAL_SHARDS);

        // Test multi-proof for a subset of chunks
        let subset_indices = vec![0, 5, 10, 15, 20];
        let subset_chunks = subset_indices.iter().map(|&i| chunks[i].clone()).collect::<Vec<_>>();
        let proof = kzg_multi_prove(&chunks, &subset_indices);
        let is_valid = kzg_multi_verify(&subset_chunks, &subset_indices, commitment, proof);
        assert!(is_valid, "Multi-proof verification should succeed for subset of chunks");
    }
}
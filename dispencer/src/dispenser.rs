use std::{collections::HashMap, iter::zip};

use anyhow::Result;
use merkle_tree::{gen_merkle_tree, MerkleProof};
use pod::{client::{PodaClientTrait, ProviderInfo}, FixedBytes, U256};
use storage_provider::http::{BatchRetrieveRequest, BatchRetrieveResponse, BatchStoreRequest};
use common::{constants::{REQUIRED_SHARDS, TOTAL_SHARDS}, log::{debug, error, info, warn}, types::Chunk};
use reed_solomon_erasure::ReedSolomon;
use sha3::{Digest, Keccak256};
use kzg::{kzg_commit, kzg_multi_prove, types::KzgProof};
type ChunkAssignment = HashMap<String, Vec<Chunk>>;

const MIN_DATA_SIZE: usize = 16;

pub struct Dispenser<T: PodaClientTrait> {
    pub pod: T,
}

impl<T: PodaClientTrait> Dispenser<T> {
    pub fn new(pod: T) -> Self {
        info!("Creating dispenser");
        Self { pod }
    }

    pub async fn submit_data(&self, data: &[u8]) -> Result<(FixedBytes<32>, ChunkAssignment)> {
        if data.len() < MIN_DATA_SIZE {
            return Err(anyhow::anyhow!("Data size is too small. Must be at least {} bytes", MIN_DATA_SIZE));
        }
        let storage_providers = self.pod.get_providers().await?.iter().map(|p| p.clone()).collect::<Vec<_>>();
        let chunks = self.erasure_encode(data, REQUIRED_SHARDS, TOTAL_SHARDS);
        let merkle_tree = gen_merkle_tree(&chunks);

        let (kzg_commitment, _) = kzg_commit(&chunks);
        let res = self.pod.submit_commitment(merkle_tree.root(), data.len() as u32, TOTAL_SHARDS as u16, REQUIRED_SHARDS as u16, kzg_commitment.try_into().unwrap()).await;
        if res.is_err() {
            error!("Failed to submit commitment: {:?}", res.err());
            return Err(anyhow::anyhow!("Failed to submit commitment. Submit already exists"));
        }
        info!("Submitted commitment");

        let assignments = self.assign_chunks(&chunks, &storage_providers)?;

        let mut promised_chunks: usize = 0;
        for (provider_id, provider_chunks) in &assignments {
            let chunk_ids = provider_chunks.iter().map(|c| c.index as usize).collect::<Vec<_>>();

            let kzg_proof = kzg_multi_prove(&chunks, &chunk_ids);
            let merkle_proofs = provider_chunks.iter().map(|c| merkle_tree::gen_proof(&merkle_tree, c.clone()).unwrap()).collect::<Vec<_>>();

            let provider = storage_providers.iter().find(|p| p.name == *provider_id).unwrap();
            let result = self.batch_submit_to_provider(provider_chunks.clone(), merkle_tree.root(), provider, kzg_proof, merkle_proofs).await;
            if result.is_err() {
                warn!("Failed to submit chunks to provider {}: {:?}", provider_id, result.err());
                continue;
            }
            promised_chunks += chunk_ids.len();
        }

        if promised_chunks < REQUIRED_SHARDS {
            return Err(anyhow::anyhow!("Not enough chunks where promised to providers"));
        }

        self.pod.wait_for_availability(merkle_tree.root()).await?;

        Ok((merkle_tree.root(), assignments))
    }

    pub async fn retrieve_data(&self, commitment: FixedBytes<32>) -> Result<Vec<u8>> {
        info!("Retrieving data for commitment: {:?}", commitment);
        let (commitment_info, is_recoverable) = self.pod.get_commitment_info(commitment).await?;
        if !is_recoverable {
            return Err(anyhow::anyhow!("Commitment is not recoverable"));
        }

        let storage_providers = self.pod.get_providers().await?.iter().map(|p| p.clone()).collect::<Vec<_>>();

        const NO_CHUNK: Option<Chunk> = None;
        let mut chunks = [NO_CHUNK; TOTAL_SHARDS];
        for provider in storage_providers {
            let chunk_ids = self.pod.get_provider_chunks(commitment, provider.addr).await?;
            debug!("Chunk ids for provider {}: {:?}", provider.name, chunk_ids);
            let provider_chunks = self.batch_retrieve_from_provider(commitment, &chunk_ids, &provider).await;
            if provider_chunks.is_err() {
                warn!("Failed to retrieve chunks from provider {}: {:?}", provider.name, provider_chunks.err());
                for chunk_id in chunk_ids {
                    chunks[chunk_id as usize] = NO_CHUNK.clone();
                }
                continue;
            }

            let provider_chunks = provider_chunks.unwrap();
            for (index, chunk) in zip(chunk_ids, provider_chunks) {
                chunks[index as usize] = chunk;
            }
        }

        let retrieved_chunks = chunks.iter().filter(|c| c.is_some()).count();
        info!("Retrieved {} chunks out of {} for commitment: {:?}", retrieved_chunks, TOTAL_SHARDS, commitment);

        if retrieved_chunks < REQUIRED_SHARDS {
            error!("Not enough chunks retrieved to reconstruct data");
            return Err(anyhow::anyhow!("Not enough chunks retrieved to reconstruct data"));
        }

        // reality check
        for (index, chunk) in chunks.iter().enumerate() {
            if chunk.is_none() {
                warn!("Chunk at index {} is none", index);
            }
            if chunk.is_some() {
                debug!("Chunk at index {} is some", index);
            }
        }

        let (data, _) = self.erasure_decode(chunks.to_vec(), REQUIRED_SHARDS, TOTAL_SHARDS, commitment_info.size as usize)?;

        Ok(data)
    }

    pub fn erasure_encode(&self, data: &[u8], required_shards: usize, total_shards: usize) -> Vec<Chunk> {
        let parity_shards = total_shards - required_shards;
        let r = ReedSolomon::<reed_solomon_erasure::galois_8::Field>::new(required_shards, parity_shards).unwrap();
        let mut master_copy = self.create_shards(data, required_shards, total_shards);

        r.encode(&mut master_copy).unwrap();

        let chunks = master_copy.iter().enumerate().map(|(index, shard)| Chunk {
            index: index as u16,
            data: shard.to_vec(),
        }).collect::<Vec<_>>();

        if chunks.len() != total_shards {
            panic!("Invalid number of chunks: {}", chunks.len());
        }

        chunks
    }

    pub fn erasure_decode(&self, chunks: Vec<Option<Chunk>>, required_shards: usize, total_shards: usize, original_length: usize) -> Result<(Vec<u8>, Vec<Chunk>)> {
        let parity_shards = total_shards - required_shards;
        let r = ReedSolomon::<reed_solomon_erasure::galois_8::Field>::new(required_shards, parity_shards).unwrap();

        // Convert chunks to shards for reconstruction
        let mut shards: Vec<Option<Vec<u8>>> = chunks.iter()
            .map(|chunk| chunk.as_ref().map(|c| c.data.clone()))
            .collect();

        debug!("Before reconstruction - shards: {:?}", shards);
        r.reconstruct(&mut shards).unwrap();
        debug!("After reconstruction - shards: {:?}", shards);

        // Get the reconstructed data chunks (first required_shards are the data shards)
        let mut reconstructed_chunks: Vec<Chunk> = Vec::new();
        let mut decoded = Vec::new();
        
        for i in 0..required_shards {
            if let Some(data) = &shards[i] {
                let chunk = Chunk {
                    index: i as u16,
                    data: data.clone(),
                };

                reconstructed_chunks.push(chunk);
                decoded.extend_from_slice(data);
            } else {
                return Err(anyhow::anyhow!("Missing data chunk after reconstruction"));
            }
        }
        
        // Trim to original length
        decoded.truncate(original_length);
        
        Ok((decoded, reconstructed_chunks))
    }

    async fn batch_retrieve_from_provider(&self, commitment: FixedBytes<32>, chunk_ids: &Vec<u16>, storage_provider: &ProviderInfo) -> Result<Vec<Option<Chunk>>> {
        let url = format!("{}/batch-retrieve", storage_provider.url);
        let body = BatchRetrieveRequest {
            commitment,
            indices: chunk_ids.clone(),
        };

        let response = reqwest::Client::new().post(url).json(&body).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to retrieve chunks: {:?}", response.text().await.unwrap()));
        }

        let message: BatchRetrieveResponse = serde_json::from_str(&response.text().await.unwrap()).unwrap();

        Ok(message.chunks)
    }

    pub async fn batch_submit_to_provider(&self, chunks: Vec<Chunk>, commitment: FixedBytes<32>, storage_provider: &ProviderInfo, proof: KzgProof, merkle_proofs: Vec<MerkleProof>) -> Result<()> {
        let url = format!("{}/batch-store", storage_provider.url);
        let body = BatchStoreRequest {
            commitment,
            chunks,
            kzg_proof: proof,
            merkle_proofs,
        };

        let response = reqwest::Client::new().post(url).json(&body).send().await?;

        if !response.status().is_success() {
            let error: serde_json::Value = serde_json::from_str(&response.text().await.unwrap()).unwrap();
            return Err(anyhow::anyhow!("Failed to submit chunks: {:?}", error["message"]));
        }

        Ok(())
    }

    fn assign_chunks(&self, chunks: &Vec<Chunk>, providers: &Vec<ProviderInfo>) -> Result<ChunkAssignment> {
        // Calculate total stake
        let total_stake = providers.iter().map(|p| p.stakedAmount).sum::<U256>();
        
        // Create assignment map
        let mut assignments: HashMap<String, Vec<Chunk>> = HashMap::with_capacity(providers.len());
        for provider in providers {
            assignments.insert(provider.name.clone(), Vec::new());
        }
        
        // Assign each chunk individually using deterministic round-robin
        for chunk in chunks {
            let provider = self.select_provider_for_chunk(
                &chunk.hash(), 
                chunk.index, 
                &providers,
                total_stake
            ).unwrap();

            if let Some(provider) = assignments.get_mut(&provider.name) {
                provider.push(chunk.clone());
            } else {
                assignments.insert(provider.name.clone(), vec![chunk.clone()]);
            }
        }
        
        Ok(assignments)
    }
    
    fn select_provider_for_chunk(&self, commitment: &FixedBytes<32>, chunk_index: u16, providers: &Vec<ProviderInfo>, total_stake: U256) -> Result<ProviderInfo> {
        // Create deterministic seed for this specific chunk
        let mut seed_input = commitment.to_vec();
        seed_input.extend_from_slice(chunk_index.to_string().as_bytes());
        let seed = Keccak256::digest(&seed_input);
        let random_value = u64::from_le_bytes(seed[0..8].try_into().unwrap()); // Use first 8 bytes
        
        // Weighted selection based on stake
        let target = U256::from(random_value) % total_stake;
        let mut cumulative_stake = U256::ZERO;
        
        for provider in providers {
            cumulative_stake += provider.stakedAmount;
            if target < cumulative_stake {
                return Ok(provider.clone());
            }
        }
        
        // Fallback (shouldn't happen)
        Ok(providers[providers.len() - 1].clone())
    }

    pub fn create_shards(&self, data: &[u8], required_shards: usize, total_shards: usize) -> Vec<Vec<u8>> {
        let parity_shards = total_shards - required_shards;

        let split_data = self.split_to_chunks(data, required_shards);
        let split_data_len = split_data[0].len();

        // add parity shareds of the same size as the data shards
        let mut shards = Vec::with_capacity(total_shards);

        // add the data shards
        shards.extend(split_data);

        // add the parity shards
        shards.extend(vec![vec![0; split_data_len]; parity_shards]);

        shards
    }

    fn split_to_chunks(&self, data: &[u8], data_shards: usize) -> Vec<Vec<u8>> {
        // Calculate chunk size, ensuring it's even
        let mut chunk_size = (data.len() + data_shards - 1) / data_shards;
        if chunk_size % 2 != 0 {
            chunk_size += 1;
        }
        
        let mut chunks = Vec::with_capacity(data_shards);
        
        for i in 0..data_shards {
            let start = i * chunk_size;
            let end = std::cmp::min(start + chunk_size, data.len());
            
            let mut chunk = vec![0u8; chunk_size];
            if start < data.len() {
                chunk[..end - start].copy_from_slice(&data[start..end]);
            }
            chunks.push(chunk);
        }

        chunks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod::{client::MockPodaClientTrait, Address, FixedBytes};
    use common::constants::REQUIRED_SHARDS;

    async fn create_test_dispenser() -> Dispenser<MockPodaClientTrait> {
        let pod = MockPodaClientTrait::new();
        Dispenser::new(pod)
    }

    fn create_test_providers() -> Vec<ProviderInfo> {
        vec![
            ProviderInfo {
                name: "Test Provider 1".to_string(),
                url: "https://test-provider-1.com".to_string(),
                addr: Address::default(),
                registeredAt: 0,
                challengeCount: 0,
                challengeSuccessCount: 0,
                active: true,
                stakedAmount: U256::from(100),
            },
            ProviderInfo {
                name: "Test Provider 2".to_string(),
                addr: Address::default(),
                url: "https://test-provider-2.com".to_string(),
                registeredAt: 0,
                challengeCount: 0,
                challengeSuccessCount: 0,
                active: true,
                stakedAmount: U256::from(200),
            },
            ProviderInfo {
                name: "Test Provider 3".to_string(),
                addr: Address::default(),
                url: "https://test-provider-3.com".to_string(),
                registeredAt: 0,
                challengeCount: 0,
                challengeSuccessCount: 0,
                active: true,
                stakedAmount: U256::from(300),
            },
        ]
    }

    #[tokio::test]
    async fn test_erasure_coding_roundtrip() {
        let dispenser = create_test_dispenser().await;
        let original_data = "Hello, this is a test message for erasure coding!".repeat(1000);
        let original_data = original_data.as_bytes();
        
        // Test encoding
        let chunks = dispenser.erasure_encode(original_data, REQUIRED_SHARDS, TOTAL_SHARDS);
        assert_eq!(chunks.len(), TOTAL_SHARDS);

        // Test decoding with all chunks
        let shards: Vec<Option<Chunk>> = chunks.into_iter()
            .map(|chunk| Some(chunk))
            .collect();
        
        let (decoded, reconstructed_chunks) = dispenser.erasure_decode(shards, REQUIRED_SHARDS, TOTAL_SHARDS, original_data.len()).unwrap();
        assert_eq!(decoded, original_data);
        assert_eq!(reconstructed_chunks.len(), REQUIRED_SHARDS);
        
        // Verify each reconstructed chunk has the correct index and hash
        for (i, chunk) in reconstructed_chunks.iter().enumerate() {
            assert_eq!(chunk.index, i as u16);
            assert_eq!(chunk.hash(), FixedBytes::from_slice(&Keccak256::digest(&chunk.data)));
        }

        // Test encoding again for the missing chunks test
        let chunks = dispenser.erasure_encode(original_data, REQUIRED_SHARDS, TOTAL_SHARDS);
        
        // Test decoding with some missing chunks
        let option_chunks: Vec<Option<Chunk>> = chunks.into_iter().map(Some).collect();
        let mut chunks_with_missing = option_chunks.clone();
        chunks_with_missing[2] = None;
        chunks_with_missing[3] = None;
        
        let (decoded_with_missing, reconstructed_chunks) = dispenser.erasure_decode(
            chunks_with_missing,
            REQUIRED_SHARDS,
            TOTAL_SHARDS,
            original_data.len()
        ).unwrap();
        assert_eq!(decoded_with_missing, original_data);
        assert_eq!(reconstructed_chunks.len(), REQUIRED_SHARDS);
        
        // Verify reconstructed chunks after missing data
        for (i, chunk) in reconstructed_chunks.iter().enumerate() {
            assert_eq!(chunk.index, i as u16);
            assert_eq!(chunk.hash(), FixedBytes::from_slice(&Keccak256::digest(&chunk.data)));
        }
    }

    #[tokio::test]
    async fn test_chunk_assignment() {
        let dispenser = create_test_dispenser().await;
        let providers = create_test_providers();
        
        // Create some test chunks
        let test_data = "Test data for chunk assignment".repeat(1000);
        let test_data = test_data.as_bytes();
        let chunks = dispenser.erasure_encode(test_data, REQUIRED_SHARDS, TOTAL_SHARDS);
        
        // Test chunk assignment
        let assignments = dispenser.assign_chunks(&chunks, &providers).unwrap();
        
        // Verify assignments
        assert_eq!(assignments.len(), providers.len());
        
        // Check that all chunks are assigned
        let total_assigned_chunks: usize = assignments.values()
            .map(|chunks| chunks.len())
            .sum();
        assert_eq!(total_assigned_chunks, TOTAL_SHARDS);
        
        // Verify each provider has at least one chunk
        for provider in &providers {
            assert!(assignments.contains_key(&provider.name));
            assert!(!assignments[&provider.name].is_empty());
        }
    }

    #[tokio::test]
    async fn test_provider_selection() {
        let dispenser = create_test_dispenser().await;
        let providers = create_test_providers();
        let total_stake: U256 = providers.iter().map(|p| p.stakedAmount).sum();
        
        // Test multiple selections to verify distribution
        let mut selections = HashMap::new();
        let test_commitment = FixedBytes::<32>::from_slice(&Keccak256::digest("test_commitment"));
        
        for i in 0..1000 {
            let provider = dispenser.select_provider_for_chunk(
                &test_commitment,
                i as u16,
                &providers,
                total_stake
            ).unwrap();
            
            *selections.entry(provider.name.clone()).or_insert(0) += 1;
        }
        
        assert_eq!(selections.len(), providers.len());
        
        for provider in &providers {
            let expected_selections = (provider.stakedAmount.as_limbs()[0] as u128 * 1000) / total_stake.as_limbs()[0] as u128;
            let actual_selections = selections.get(&provider.name).unwrap();
            let variance = (expected_selections as i32 - *actual_selections).abs();
            
            // Allow for 20% variance
            assert!(variance <= (expected_selections as f64 * 0.2) as i32,
                "Provider {} had {} selections, expected {}",
                provider.name, actual_selections, expected_selections);
        }
    }

    #[tokio::test]
    async fn test_create_shards() {
        let dispenser = create_test_dispenser().await;
        let test_data = b"Test data for shard creation";
        
        let shards = dispenser.create_shards(test_data, REQUIRED_SHARDS, TOTAL_SHARDS);
        
        // Verify shard count
        assert_eq!(shards.len(), TOTAL_SHARDS);
        
        // Verify all shards have the same length
        let shard_length = shards[0].len();
        for shard in &shards {
            assert_eq!(shard.len(), shard_length);
        }
        
        // Verify data shards contain the original data
        let mut reconstructed = Vec::new();
        for shard in shards.iter().take(REQUIRED_SHARDS) {
            reconstructed.extend_from_slice(shard);
        }
        reconstructed.truncate(test_data.len());
        assert_eq!(&reconstructed, test_data);
    }
}
#[cfg(test)]
mod tests {
    use crate::setup;

    use dispencer::{
        http::{RetrieveDataRequest, RetrieveDataResponse, SubmitDataRequest, SubmitDataResponse}
    };
    use merkle_tree::MerkleProof;
    use pod::{client::{PodaClient, PodaClientTrait}, Address, FixedBytes, PrivateKeySigner, U256};
    use reqwest::Response;
    use types::{constants::{ONE_ETH, REQUIRED_SHARDS, TOTAL_SHARDS}, log::info, Chunk};
    use kzg::types::{KzgCommitment, KzgProof};
    use anyhow::Result;
    use setup::setup::{setup_pod, Setup};
    use storage_provider::{responder::respond_to_active_challenges, storage::ChunkStorageTrait};
    use ark_bls12_381::G1Projective as G1;
    use ark_std::UniformRand;

    const RPC_URL: &str = "http://localhost:8545";
    const N_STORAGE_PROVIDERS: usize = 3;

    async fn check_health(url: &str, path: &str) -> Result<Response, reqwest::Error> {
        let client = reqwest::Client::new();
        let url = format!("{}/{}", url, path);
        client.get(&url).send().await
    }

    async fn delete_provider_chunk(provider_url: &str, commitment: &FixedBytes<32>, chunks: &Vec<u16>) -> Result<Response, reqwest::Error> {
        let client = reqwest::Client::new();
        let url = format!("{}/delete", provider_url);

        client.post(&url).json(&serde_json::json!({
            "commitment": commitment,
            "indices": chunks
        })).send().await
    }

    async fn submit_data(dispencer_url: &str, data: &[u8]) -> Result<Response, reqwest::Error> {
        let client = reqwest::Client::new();
        let url = format!("{}/submit", dispencer_url);
        let request_body = SubmitDataRequest {
            data: data.to_vec(),
        };

        client.post(&url).json(&request_body).send().await
    }

    async fn retrieve_data(dispencer_url: &str, commitment: &FixedBytes<32>) -> Result<Vec<u8>> {
        let client = reqwest::Client::new();
        let url = format!("{}/retrieve", dispencer_url);
        let request_body = RetrieveDataRequest {
            commitment: commitment.clone(),
        };

        let response = client.post(&url).json(&request_body).send().await?;
        let status = response.status();
        let response_body: RetrieveDataResponse = response.json().await?;
        if !status.is_success() || !response_body.success || response_body.data.is_none() {
            return Err(anyhow::anyhow!(response_body.message));
        }

        Ok(response_body.data.unwrap())
    }

    async fn get_view_poda_client(poda_address: Address) -> PodaClient {
        let random_signer = PrivateKeySigner::random();
        PodaClient::new(random_signer, RPC_URL.to_string(), poda_address).await
    }

    #[tokio::test]
    async fn test_setup() {
        let Setup { poda_address, dispencer_handle, storage_server_handles, challenger: _ } = setup_pod(N_STORAGE_PROVIDERS, RPC_URL, false).await;
        let random_signer = PrivateKeySigner::random();
        let poda_client = PodaClient::new(random_signer, RPC_URL.to_string(), poda_address).await;

        let health_response = check_health(&dispencer_handle.base_url, "health").await.unwrap();
        if !health_response.status().is_success() {
            panic!("Dispencer health check failed: {}", health_response.text().await.unwrap());
        }

        let providers = poda_client.get_providers().await.unwrap();
        for (i, provider) in providers.iter().enumerate() {
            let provider_url = provider.url.as_str();
            assert_eq!(*provider_url, storage_server_handles[i].base_url);
            let response = check_health(provider_url, "health").await.unwrap();
            if !response.status().is_success() {
                panic!("Provider health check failed: {}", response.text().await.unwrap());
            }
        }
    }

    #[tokio::test]
    async fn test_store_data() {
        #[allow(unused_variables)]
        let Setup { poda_address, dispencer_handle, storage_server_handles, challenger: _ } = setup_pod(N_STORAGE_PROVIDERS, RPC_URL, false).await;
        let random_signer = PrivateKeySigner::random();
        let poda_client = PodaClient::new(random_signer, RPC_URL.to_string(), poda_address).await;

        let data = b"hello, world".repeat(10);

        let assignments = submit_data(&dispencer_handle.base_url, &data).await.unwrap();
        if !assignments.status().is_success() {
            panic!("Failed to submit data: {}", assignments.text().await.unwrap());
        }
        let result: SubmitDataResponse = assignments.json().await.unwrap();

        let (commitment_info, is_recoverable) = poda_client.get_commitment_info(result.commitment).await.unwrap();
        assert_eq!(commitment_info.availableChunks, TOTAL_SHARDS as u16);
        assert_eq!(commitment_info.totalChunks, TOTAL_SHARDS as u16);
        assert_eq!(commitment_info.requiredChunks, REQUIRED_SHARDS as u16);
        assert_eq!(commitment_info.size, data.len() as u32);
        assert!(is_recoverable);

        let providers = poda_client.get_eligible_providers().await.unwrap();
        for provider in providers {
            let provider_chunks = poda_client.get_provider_chunks(result.commitment, provider.addr).await.unwrap();
            let assignment = result.assignments.get(&provider.name).unwrap();


            for chunk in assignment {
                assert!(provider_chunks.contains(chunk));
            }
        }
    }

    #[tokio::test]
    async fn test_retrieve_data() {
        #[allow(unused_variables)]
        let Setup { poda_address, dispencer_handle, storage_server_handles, challenger: _ } = setup_pod(N_STORAGE_PROVIDERS, RPC_URL, false).await;

        let data = b"hello, world".repeat(10);

        let response = submit_data(&dispencer_handle.base_url, &data).await.unwrap();
        if !response.status().is_success() {
            panic!("Failed to submit data: {}", response.text().await.unwrap());
        }
        let result: SubmitDataResponse = response.json().await.unwrap();

        let retrieve_data = retrieve_data(&dispencer_handle.base_url, &result.commitment).await.unwrap();

        assert_eq!(retrieve_data, data);
    }

    #[tokio::test]
    async fn test_retrieve_some_data() {
        #[allow(unused_variables)]
        let Setup { poda_address, dispencer_handle, storage_server_handles, challenger: _ } = setup_pod(N_STORAGE_PROVIDERS, RPC_URL, false).await;
        let poda_client = get_view_poda_client(poda_address).await;

        let data = b"hello, world".repeat(10);

        let response = submit_data(&dispencer_handle.base_url, &data).await.unwrap();
        if !response.status().is_success() {
            panic!("Failed to submit data: {}", response.text().await.unwrap());
        }
        let result: SubmitDataResponse = response.json().await.unwrap();

        let providers = poda_client.get_providers().await.unwrap();
        for (provider_name, chunks) in result.assignments.iter() {
            let provider = providers.iter().find(|p| p.name == *provider_name).unwrap();
            let chunk_index = chunks.first().unwrap();
            delete_provider_chunk(provider.url.as_str(), &result.commitment, &vec![*chunk_index]).await.unwrap();
        }

        let retrieve_data = retrieve_data(&dispencer_handle.base_url, &result.commitment).await.unwrap();

        assert_eq!(retrieve_data, data);
    }

    #[tokio::test]
    async fn test_retrieve_no_data() {
        #[allow(unused_variables)]
        let Setup { poda_address, dispencer_handle, storage_server_handles, challenger: _ } = setup_pod(N_STORAGE_PROVIDERS, RPC_URL, false).await;
        let poda_client = get_view_poda_client(poda_address).await;

        let data = b"hello, world".repeat(10);

        let response = submit_data(&dispencer_handle.base_url, &data).await.unwrap();
        if !response.status().is_success() {
            panic!("Failed to submit data: {}", response.text().await.unwrap());
        }
        let result: SubmitDataResponse = response.json().await.unwrap();

        let providers = poda_client.get_providers().await.unwrap();
        let mut to_delete: usize = 9;
        for (provider_name, chunks) in result.assignments.iter() {
            let provider = providers.iter().find(|p| p.name == *provider_name).unwrap();
            let to_delete_chunks = chunks.iter().take(to_delete).map(|c| *c).collect::<Vec<_>>();
            delete_provider_chunk(provider.url.as_str(), &result.commitment, &to_delete_chunks).await.unwrap();
            to_delete -= to_delete_chunks.len();
            if to_delete == 0 {
                break;
            }
        }

        let retrieve_data = retrieve_data(&dispencer_handle.base_url, &result.commitment).await;

        match retrieve_data {
            Ok(data) => panic!("Retrieved data: {:?}", data),
            Err(e) => assert_eq!(e.to_string(), "Failed to retrieve data: Not enough chunks retrieved to reconstruct data"),
        }
    }

    #[tokio::test]
    async fn test_invalid_kzg_commitment() {
        #[allow(unused_variables)]
        let Setup { poda_address, dispencer_handle, storage_server_handles, challenger: _ } = setup_pod(N_STORAGE_PROVIDERS, RPC_URL, false).await;

        let data = b"hello, world".repeat(10);
        let chunks = dispencer_handle.dispencer.erasure_encode(&data, REQUIRED_SHARDS, TOTAL_SHARDS);
        let merkle_tree = merkle_tree::gen_merkle_tree(&chunks);

        let mut rng = ark_std::test_rng();
        let invalid_g1_point = G1::rand(&mut rng);
        let invalid_kzg_commitment = KzgCommitment::new(invalid_g1_point);
        
        dispencer_handle.dispencer.pod.submit_commitment(merkle_tree.root(), data.len() as u32, TOTAL_SHARDS as u16, REQUIRED_SHARDS as u16, invalid_kzg_commitment.try_into().unwrap()).await.unwrap();
        let providers = dispencer_handle.dispencer.pod.get_providers().await.unwrap();

        let mut rng = ark_std::test_rng();
        let another_invalid_g1_point = G1::rand(&mut rng);
        let proof = KzgProof::new(another_invalid_g1_point);
        let merkle_proofs = chunks.iter().map(|c| merkle_tree::gen_proof(&merkle_tree, c.clone()).unwrap()).collect::<Vec<_>>();

        let result = dispencer_handle.dispencer.batch_submit_to_provider(chunks, merkle_tree.root(), &providers[0], proof, merkle_proofs).await;
        if result.is_ok() {
            panic!("Should have failed to submit chunks");
        }
    }

    #[tokio::test]
    async fn test_verify_chunk_proofs() {
        let Setup { poda_address: _, dispencer_handle, storage_server_handles: _, challenger: _ } = setup_pod(N_STORAGE_PROVIDERS, RPC_URL, false).await;

        let chunks: Vec<Chunk> = vec![
            Chunk { index: 0, data: b"hello".to_vec() },
            Chunk { index: 1, data: b"world".to_vec() },
            Chunk { index: 2, data: b"hello".to_vec() },
            Chunk { index: 3, data: b"world".to_vec() }
        ];

        let tree = merkle_tree::gen_merkle_tree(&chunks);
        
        let root = tree.root();

        for chunk in &chunks {
            let proof = merkle_tree::gen_proof(&tree, chunk.clone()).unwrap();
            let result = dispencer_handle.dispencer.pod.verify_chunk_proof(proof.path.clone(), root, chunk.index, chunk.data.clone().into()).await.unwrap();
            assert_eq!(result, true);
        }

        let invalid_proof = MerkleProof {
            path: vec![tree.root()],
        };
        let result = dispencer_handle.dispencer.pod.verify_chunk_proof(invalid_proof.path.clone(), root, 0, chunks[0].clone().data.into()).await.unwrap();
        assert_eq!(result, false);

        let proof = merkle_tree::gen_proof(&tree, chunks[0].clone()).unwrap();
        let result = dispencer_handle.dispencer.pod.verify_chunk_proof(proof.path.clone(), root, 1, chunks[0].clone().data.into()).await.unwrap();
        assert_eq!(result, false);

        let proof = merkle_tree::gen_proof(&tree, chunks[0].clone()).unwrap();
        let result = dispencer_handle.dispencer.pod.verify_chunk_proof(proof.path.clone(), root, 0, chunks[1].clone().data.into()).await.unwrap();
        assert_eq!(result, false);

        let proof = merkle_tree::gen_proof(&tree, chunks[0].clone()).unwrap();
        let result = dispencer_handle.dispencer.pod.verify_chunk_proof(proof.path.clone(), root, 0, chunks[0].clone().data.into()).await.unwrap();
        assert_eq!(result, true);
    }

    #[tokio::test]
    async fn test_sampling() {
        let Setup { poda_address, dispencer_handle, storage_server_handles, challenger } = setup_pod(N_STORAGE_PROVIDERS, RPC_URL, true).await;
        let challenger = challenger.unwrap();
        let poda_client = get_view_poda_client(poda_address).await;

        let data = b"hello, world".repeat(10);
        submit_data(&dispencer_handle.base_url, &data).await.unwrap();

        let challenges = challenger.sample_challenges(10).await.unwrap();

        for challenge in challenges {
            let (challenge_id, commitment, chunk_id, provider) = challenge;
            let challenge_info = poda_client.get_chunk_challenge(commitment, chunk_id, provider).await.unwrap();

            info!("Challenge info: {:?}", challenge_info);
            assert_eq!(challenge_info.challenge.challengeId, challenge_id);

            let provider = storage_server_handles.iter().find(|p| p.owner_address == provider).unwrap();
            let response = provider.storage.retrieve(commitment, chunk_id).await.unwrap();
            if response.is_none() {
                panic!("Failed to retrieve chunk");
            }
        }

        for storage_server_handle in storage_server_handles {
            respond_to_active_challenges(&storage_server_handle.storage, &storage_server_handle.pod, storage_server_handle.owner_address).await.unwrap();
            let active_challenges = dispencer_handle.dispencer.pod.get_provider_active_challenges(storage_server_handle.owner_address).await.unwrap();
            assert_eq!(active_challenges.len(), 0);

            // make sure no slashing happened
            let provider_info = storage_server_handle.pod.get_provider_info(storage_server_handle.owner_address).await.unwrap();
            assert_eq!(provider_info.stakedAmount, U256::from(ONE_ETH));
        }
    }

    #[tokio::test]
    async fn test_slashed_for_wrong_data() {
        let Setup { poda_address: _, dispencer_handle, storage_server_handles, challenger } = setup_pod(N_STORAGE_PROVIDERS, RPC_URL, true).await;
        let challenger = challenger.unwrap();

        let data = b"hello, world".repeat(10);
        let res = submit_data(&dispencer_handle.base_url, &data).await.unwrap();
        let result: SubmitDataResponse = res.json().await.unwrap();

        let random_index = rand::random_range(0..storage_server_handles.len());
        let provider = storage_server_handles.get(random_index).unwrap();
        let assigments_of_provider = result.assignments.get(&provider.name).unwrap();
        let random_index = rand::random_range(0..assigments_of_provider.len());
        let chunk_id = assigments_of_provider[random_index];

        let challenge = challenger.pod.issue_chunk_challenge(result.commitment, chunk_id, provider.owner_address).await.unwrap();
        info!("Challenge issued: {:?}", challenge);

        let chunk_with_proof = provider.storage.retrieve(result.commitment, chunk_id).await.unwrap().unwrap();

        let other_data = b"world, hello".repeat(10);
        let response = provider.pod.respond_to_chunk_challenge(result.commitment, chunk_id, other_data.clone().into(), chunk_with_proof.1.path.clone()).await;
        if response.is_err() {
            panic!("Failed to respond to challenge: {:?}", response.err());
        }

        let (commitment_info, is_recoverable) = provider.pod.get_commitment_info(result.commitment).await.unwrap();
        assert_eq!(commitment_info.availableChunks, commitment_info.totalChunks as u16 - 1);
        assert_eq!(is_recoverable, true);

        let provider_info = provider.pod.get_provider_info(provider.owner_address).await.unwrap();
        assert_eq!(provider_info.stakedAmount, U256::from(ONE_ETH) - U256::from(ONE_ETH / 10));
        assert_eq!(provider_info.active, true);
        assert_eq!(provider_info.challengeSuccessCount, provider_info.challengeCount - 1);
    }
}

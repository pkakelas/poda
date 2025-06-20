#[allow(unused_imports, dead_code)]
#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::setup;

    use dispencer::{
        dispenser::Dispenser, http::{RetrieveDataRequest, RetrieveDataResponse, SubmitDataRequest, SubmitDataResponse}
    };
    use merkle_tree::MerkleProof;
    use pod::{client::{PodaClient, PodaClientTrait}, FixedBytes, PrivateKeySigner};
    use reqwest::Response;
    use types::{constants::{REQUIRED_SHARDS, TOTAL_SHARDS}, Chunk};
    use kzg::types::{KzgCommitment, KzgProof};
    use anyhow::Result;
    use sha3::{Digest, Keccak256};
    use setup::setup::{setup_pod, Setup};

    // Create an invalid commitment by using a different random G1 point
    use ark_bls12_381::G1Projective as G1;
    use ark_std::UniformRand;

    const RPC_URL: &str = "http://localhost:8545";
    const N_ACTORS: usize = 4; // 1 dispencer + 3 storage providers

    async fn check_health(url: &str, path: &str) -> Result<Response, reqwest::Error> {
        let client = reqwest::Client::new();
        let url = format!("{}/{}", url, path);
        client.get(&url).send().await
    }

    async fn delete_provider_chunk(provider_url: &str, namespace: &str, commitment: &FixedBytes<32>, chunks: &Vec<u16>) -> Result<Response, reqwest::Error> {
        let client = reqwest::Client::new();
        let url = format!("{}/delete", provider_url);

        client.post(&url).json(&serde_json::json!({
            "namespace": namespace,
            "commitment": commitment,
            "indices": chunks
        })).send().await
    }

    async fn submit_data(dispencer_url: &str, namespace: &str, data: &[u8]) -> Result<Response, reqwest::Error> {
        let client = reqwest::Client::new();
        let url = format!("{}/submit", dispencer_url);
        let request_body = SubmitDataRequest {
            namespace: namespace.to_string(),
            data: data.to_vec(),
        };

        client.post(&url).json(&request_body).send().await
    }

    async fn retrieve_data(dispencer_url: &str, namespace: &str, commitment: &FixedBytes<32>) -> Result<Vec<u8>> {
        let client = reqwest::Client::new();
        let url = format!("{}/retrieve", dispencer_url);
        let request_body = RetrieveDataRequest {
            namespace: namespace.to_string(),
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

    #[tokio::test]
    async fn test_setup() {
        let Setup { poda_address, dispencer_handle, dispencer_client: _, storage_server_handles } = setup_pod(N_ACTORS, RPC_URL).await;
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
        let Setup { poda_address, dispencer_handle, dispencer_client, storage_server_handles } = setup_pod(N_ACTORS, RPC_URL).await;
        let random_signer = PrivateKeySigner::random();
        let poda_client = PodaClient::new(random_signer, RPC_URL.to_string(), poda_address).await;

        let namespace = "test_namespace";
        let data = b"hello, world".repeat(10);
        let commitment = FixedBytes::from_slice(&Keccak256::digest(&data));

        let assignments = submit_data(&dispencer_handle.base_url, namespace, &data).await.unwrap();
        if !assignments.status().is_success() {
            panic!("Failed to submit data: {}", assignments.text().await.unwrap());
        }
        let assignments: SubmitDataResponse = assignments.json().await.unwrap();

        let (commitment_info, is_recoverable) = poda_client.get_commitment_info(commitment).await.unwrap();
        assert_eq!(commitment_info.availableChunks, TOTAL_SHARDS as u16);
        assert_eq!(commitment_info.totalChunks, TOTAL_SHARDS as u16);
        assert_eq!(commitment_info.requiredChunks, REQUIRED_SHARDS as u16);
        assert_eq!(commitment_info.size, data.len() as u32);
        assert!(is_recoverable);

        let providers = poda_client.get_eligible_providers().await.unwrap();
        for provider in providers {
            let provider_chunks = poda_client.get_provider_chunks(commitment, provider.addr).await.unwrap();
            let assignment = assignments.assignments.get(&provider.name).unwrap();


            for chunk in assignment {
                assert!(provider_chunks.contains(chunk));
            }
        }
    }

    #[tokio::test]
    async fn test_retrieve_data() {
        #[allow(unused_variables)]
        let Setup { poda_address, dispencer_handle, dispencer_client, storage_server_handles } = setup_pod(N_ACTORS, RPC_URL).await;

        let namespace = "test_namespace";
        let data = b"hello, world".repeat(10);
        let commitment = FixedBytes::from_slice(&Keccak256::digest(&data));

        let _ = submit_data(&dispencer_handle.base_url, namespace, &data).await.unwrap();

        let retrieve_data = retrieve_data(&dispencer_handle.base_url, namespace, &commitment).await.unwrap();

        assert_eq!(retrieve_data, data);
    }

    #[tokio::test]
    async fn test_retrieve_some_data() {
        #[allow(unused_variables)]
        let Setup { poda_address, dispencer_handle, dispencer_client, storage_server_handles } = setup_pod(N_ACTORS, RPC_URL).await;
        let random_signer = PrivateKeySigner::random();
        let poda_client = PodaClient::new(random_signer, RPC_URL.to_string(), poda_address).await;

        let namespace = "test_namespace";
        let data = b"hello, world".repeat(10);
        let commitment = FixedBytes::from_slice(&Keccak256::digest(&data));

        let response = submit_data(&dispencer_handle.base_url, namespace, &data).await.unwrap();
        if !response.status().is_success() {
            panic!("Failed to submit data: {}", response.text().await.unwrap());
        }
        let assignments: SubmitDataResponse = response.json().await.unwrap();

        let providers = poda_client.get_providers().await.unwrap();
        for (provider_name, chunks) in assignments.assignments.iter() {
            let provider = providers.iter().find(|p| p.name == *provider_name).unwrap();
            let chunk_index = chunks.first().unwrap();
            delete_provider_chunk(provider.url.as_str(), namespace, &commitment, &vec![*chunk_index]).await.unwrap();
        }

        let retrieve_data = retrieve_data(&dispencer_handle.base_url, namespace, &commitment).await.unwrap();

        assert_eq!(retrieve_data, data);
    }

    #[tokio::test]
    async fn test_retrieve_no_data() {
        #[allow(unused_variables)]
        let Setup { poda_address, dispencer_handle, dispencer_client, storage_server_handles } = setup_pod(N_ACTORS, RPC_URL).await;
        let random_signer = PrivateKeySigner::random();
        let poda_client = PodaClient::new(random_signer, RPC_URL.to_string(), poda_address).await;

        let namespace = "test_namespace";
        let data = b"hello, world".repeat(10);
        let commitment = FixedBytes::from_slice(&Keccak256::digest(&data));

        let response = submit_data(&dispencer_handle.base_url, namespace, &data).await.unwrap();
        if !response.status().is_success() {
            panic!("Failed to submit data: {}", response.text().await.unwrap());
        }
        let assignments: SubmitDataResponse = response.json().await.unwrap();

        let providers = poda_client.get_providers().await.unwrap();
        let mut to_delete: usize = 9;
        for (provider_name, chunks) in assignments.assignments.iter() {
            let provider = providers.iter().find(|p| p.name == *provider_name).unwrap();
            let to_delete_chunks = chunks.iter().take(to_delete).map(|c| *c).collect::<Vec<_>>();
            delete_provider_chunk(provider.url.as_str(), namespace, &commitment, &to_delete_chunks).await.unwrap();
            to_delete -= to_delete_chunks.len();
            if to_delete == 0 {
                break;
            }
        }

        let retrieve_data = retrieve_data(&dispencer_handle.base_url, namespace, &commitment).await;

        match retrieve_data {
            Ok(data) => panic!("Retrieved data: {:?}", data),
            Err(e) => assert_eq!(e.to_string(), "Failed to retrieve data: Not enough chunks retrieved to reconstruct data"),
        }
    }

    #[tokio::test]
    async fn test_invalid_kzg_commitment() {
        #[allow(unused_variables)]
        let Setup { poda_address, dispencer_handle, dispencer_client, storage_server_handles } = setup_pod(N_ACTORS, RPC_URL).await;
        let dispencer = Dispenser::new(dispencer_client.clone());

        let namespace = "test_namespace";
        let data = b"hello, world".repeat(10);
        let commitment: FixedBytes<32> = FixedBytes::from_slice(&Keccak256::digest(&data));


        let mut rng = ark_std::test_rng();
        let invalid_g1_point = G1::rand(&mut rng);
        let invalid_kzg_commitment = KzgCommitment::new(invalid_g1_point);
        
        dispencer_client.submit_commitment(commitment, namespace.to_string(), data.len() as u32, TOTAL_SHARDS as u16, REQUIRED_SHARDS as u16, invalid_kzg_commitment.try_into().unwrap()).await.unwrap();

        let chunks = dispencer.erasure_encode(&data, REQUIRED_SHARDS, TOTAL_SHARDS);
        let providers = dispencer_client.get_providers().await.unwrap();

        let mut rng = ark_std::test_rng();
        let another_invalid_g1_point = G1::rand(&mut rng);
        let proof = KzgProof::new(another_invalid_g1_point);

        let result = dispencer.batch_submit_to_provider(chunks, namespace.to_string(), commitment, &providers[0], proof).await;
        if result.is_ok() {
            panic!("Should have failed to submit chunks");
        }
    }

    #[tokio::test]
    async fn test_verify_chunk_proofs() {
        let Setup { poda_address: _, dispencer_handle: _, dispencer_client, storage_server_handles: _ } = setup_pod(N_ACTORS, RPC_URL).await;

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
            let result = dispencer_client.verify_chunk_proof(proof.path.clone(), root, chunk.index, chunk.data.clone().into()).await.unwrap();
            assert_eq!(result, true);
        }

        let invalid_proof = MerkleProof {
            path: vec![tree.root()],
        };
        let result = dispencer_client.verify_chunk_proof(invalid_proof.path.clone(), root, 0, chunks[0].clone().data.into()).await.unwrap();
        assert_eq!(result, false);

        let proof = merkle_tree::gen_proof(&tree, chunks[0].clone()).unwrap();
        let result = dispencer_client.verify_chunk_proof(proof.path.clone(), root, 1, chunks[0].clone().data.into()).await.unwrap();
        assert_eq!(result, false);

        let proof = merkle_tree::gen_proof(&tree, chunks[0].clone()).unwrap();
        let result = dispencer_client.verify_chunk_proof(proof.path.clone(), root, 0, chunks[1].clone().data.into()).await.unwrap();
        assert_eq!(result, false);

        let proof = merkle_tree::gen_proof(&tree, chunks[0].clone()).unwrap();
        let result = dispencer_client.verify_chunk_proof(proof.path.clone(), root, 0, chunks[0].clone().data.into()).await.unwrap();
        assert_eq!(result, true);
    }
}






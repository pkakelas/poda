#[allow(unused_imports, dead_code)]
mod tests {
    use dispencer::{
        dispenser::{REQUIRED_SHARDS, TOTAL_SHARDS},
        http::{RetrieveDataRequest, RetrieveDataResponse, SubmitDataRequest, SubmitDataResponse},
    };
    use pod::{client::{PodaClient, PodaClientTrait}, FixedBytes, PrivateKeySigner};
    use anyhow::Result;
    use sha3::{Digest, Keccak256};
    use crate::setup::setup_pod;

    const RPC_URL: &str = "http://localhost:8545";
    const N_ACTORS: usize = 4; // 1 dispencer + 3 storage providers

    async fn check_health(url: &str, path: &str) -> Result<serde_json::Value> {
        let client = reqwest::Client::new();
        let url = format!("{}/{}", url, path);
        let response = client.get(&url).send().await?;
        Ok(response.json().await?)
    }

    async fn delete_provider_chunk(provider_url: &str, namespace: &str, commitment: &FixedBytes<32>, chunks: &Vec<u16>) -> Result<()> {
        let client = reqwest::Client::new();
        let url = format!("{}/delete", provider_url);
        let response = client.post(&url).json(&serde_json::json!({
            "namespace": namespace,
            "commitment": commitment,
            "indices": chunks
        })).send().await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Failed to delete chunk"))
        }
    }

    async fn submit_data(dispencer_url: &str, namespace: &str, data: &[u8]) -> Result<SubmitDataResponse> {
        let client = reqwest::Client::new();
        let url = format!("{}/submit", dispencer_url);
        let request_body = SubmitDataRequest {
            namespace: namespace.to_string(),
            data: data.to_vec(),
        };

        let response = client.post(&url).json(&request_body).send().await?;

        if response.status().is_success() {
            Ok(response.json().await?)
        } else {
            Err(anyhow::anyhow!("Failed to submit data"))
        }
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
        let (poda_address, dispencer_handle, storage_serve_handles) = setup_pod(N_ACTORS, RPC_URL).await;
        let random_signer = PrivateKeySigner::random();
        let poda_client = PodaClient::new(random_signer, RPC_URL.to_string(), poda_address).await;

        let health_response = check_health(&dispencer_handle.base_url, "health").await.unwrap();
        assert_eq!(health_response["status"], "ok");

        let providers = poda_client.get_providers().await.unwrap();
        for (i, provider) in providers.iter().enumerate() {
            let provider_url = provider.url.as_str();
            assert_eq!(*provider_url, storage_serve_handles[i].base_url);
            let response = check_health(provider_url, "health").await.unwrap();
            assert_eq!(response["status"], "ok");
        }
    }

    #[tokio::test]
    async fn test_store_data() {
        #[allow(unused_variables)]
        let (poda_address, dispencer_handle, server_handles) = setup_pod(N_ACTORS, RPC_URL).await;
        let random_signer = PrivateKeySigner::random();
        let poda_client = PodaClient::new(random_signer, RPC_URL.to_string(), poda_address).await;

        let namespace = "test_namespace";
        let data = b"hello, world".repeat(10);
        let commitment = FixedBytes::from_slice(&Keccak256::digest(&data));

        let assignments = submit_data(&dispencer_handle.base_url, namespace, &data).await.unwrap();

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
        let (poda_address, dispencer_handle, server_handles) = setup_pod(N_ACTORS, RPC_URL).await;

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
        let (poda_address, dispencer_handle, server_handles) = setup_pod(N_ACTORS, RPC_URL).await;
        let random_signer = PrivateKeySigner::random();
        let poda_client = PodaClient::new(random_signer, RPC_URL.to_string(), poda_address).await;

        let namespace = "test_namespace";
        let data = b"hello, world".repeat(10);
        let commitment = FixedBytes::from_slice(&Keccak256::digest(&data));

        let response = submit_data(&dispencer_handle.base_url, namespace, &data).await.unwrap();

        let providers = poda_client.get_providers().await.unwrap();
        for (provider_name, chunks) in response.assignments.iter() {
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
        let (poda_address, dispencer_handle, server_handles) = setup_pod(N_ACTORS, RPC_URL).await;
        let random_signer = PrivateKeySigner::random();
        let poda_client = PodaClient::new(random_signer, RPC_URL.to_string(), poda_address).await;

        let namespace = "test_namespace";
        let data = b"hello, world".repeat(10);
        let commitment = FixedBytes::from_slice(&Keccak256::digest(&data));

        let response = submit_data(&dispencer_handle.base_url, namespace, &data).await.unwrap();

        let providers = poda_client.get_providers().await.unwrap();
        let mut to_delete: usize = 9;
        for (provider_name, chunks) in response.assignments.iter() {
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
}





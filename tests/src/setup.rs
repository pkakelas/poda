use std::fs;
use std::net::TcpListener;
use std::time::Duration;
use std::{str::FromStr, sync::Arc};
use dispencer::dispenser::Dispenser;
use pod::client::PodaClientTrait;
use pod::{Address, EthereumWallet, PodProvider, PodProviderBuilder, Provider, U256};
use pod::{client::PodaClient, PrivateKeySigner};
use serde::Deserialize;
use storage_provider::{start_server, FileStorage};
use tempfile::TempDir;
use tokio::sync::oneshot;
use tokio::time::sleep;

// Keep track of running servers to prevent them from being dropped
pub struct ServerHandle {
    _temp_dir: TempDir,
    _shutdown_tx: oneshot::Sender<()>,
    pub base_url: String,
}

// struct TestServer {
//     _temp_dir: TempDir,
//     client: reqwest::Client,
//     base_url: String,
//     _shutdown: oneshot::Sender<()>,
// }

#[derive(Deserialize, Debug)]
pub struct Actor {
    address: Address,
    private_key: String,
}

const FAUCET_PRIVATE_KEY: &str = "6df79891f22b0f3c9e9fb53b966a8861fd6fef69f99772c5c4dbcf303f10d901";
const ONE_ETH: u128 = 1000000000000000000;
const MIN_STAKE: u128 = ONE_ETH / 100;

// n_actors: Number of actors in setup. 1 will be dispencer, the rest will be storage providers
pub async fn setup_pod(n_actors: usize, rpc_url: &str) -> (Dispenser<PodaClient>, Address, Vec<ServerHandle>) {
    println!("Setting up pod");
    let faucet = PrivateKeySigner::from_str(FAUCET_PRIVATE_KEY).expect("Invalid private key");
    let faucet_address = faucet.address();
    let faucet = get_provider_for_signer(faucet, rpc_url).await;

    println!("Deploying poda contract");
    let poda_address = PodaClient::deploy_poda(faucet.clone(), faucet_address, MIN_STAKE).await.unwrap();
    println!("Deployed poda contract at: {:?}", poda_address);

    let actors = get_actors();
    println!("Fauceting actors");
    faucet_if_needed(faucet, &actors).await;

    let mut clients: Vec<PodaClient> = Vec::new();
    for actor in actors.iter() {
        let signer = PrivateKeySigner::from_str(&actor.private_key).expect("Invalid private key");
        let client = PodaClient::new(signer, rpc_url.to_string(), poda_address).await;
        clients.push(client);
    }

    // Make the first one a dispencer
    let dispencer_client = clients[0].clone();
    let dispencer = Dispenser::new(dispencer_client.clone());

    let mut server_handles: Vec<ServerHandle> = Vec::new();
    
    for i in 1..n_actors {
        let storage_provider = clients[i].clone();
        let handle = start_new_storage_provider_server(&storage_provider).await;
        let res = storage_provider.register_provider(format!("storage-provider-{}", i), handle.base_url.to_string(), ONE_ETH / 100).await;

        if res.is_err() {
            println!("Error registering provider. Probably already registered.");
        }

        println!("Storage provider url: {:?}", handle.base_url);
        server_handles.push(handle);
    }

    let providers = dispencer_client.get_providers().await.unwrap();
    println!("Providers: {:?}", providers);

    (dispencer, poda_address, server_handles)
}

pub async fn get_provider_for_signer(signer: PrivateKeySigner, rpc_url: &str) -> PodProvider {
    PodProviderBuilder::with_recommended_settings()
        .wallet(EthereumWallet::new(signer))
        .on_url(rpc_url.to_string())
        .await
        .expect("Failed to create provider")
}

pub fn get_actors() -> Vec<Actor> {
    let actors = fs::read_to_string("src/actors.json").unwrap();
    let actors: Vec<Actor> = serde_json::from_str(&actors).unwrap();
    actors
}

async fn faucet_if_needed(faucet: PodProvider, actors: &Vec<Actor>) -> () {
    for actor in actors {
        let min_balance = U256::from(ONE_ETH).div_ceil(U256::from(10)); // 0.1 eth
        let balance = faucet.get_balance(actor.address).await.unwrap();

        if balance < min_balance {
            faucet.transfer(actor.address, U256::from(ONE_ETH)).await.unwrap();
        }

        let balance = faucet.get_balance(actor.address).await.unwrap();
        println!("balance of actor {:?} is {:?}", actor.address, balance);
    }


}

async fn start_new_storage_provider_server(pod: &PodaClient) -> ServerHandle {
    // Find an available port
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener); // Close the listener so the port is free

    // Create a temporary directory for storage
    let temp_dir = TempDir::new().unwrap();
    let storage = FileStorage::new(temp_dir.path());
    let storage = Arc::new(storage);

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    // Start the server in the background
    let server = start_server(storage, Arc::new(pod.clone()), port);
    let _ = tokio::spawn(async move {
        let server = server;
        tokio::select! {
            _ = server => {},
            _ = shutdown_rx => {},
        }
    });

    // Wait for server to start
    sleep(Duration::from_millis(100)).await;

    let base_url = format!("http://localhost:{}", port);

    ServerHandle {
        base_url: base_url,
        _temp_dir: temp_dir,
        _shutdown_tx: shutdown_tx,
    }
}

//     async fn batch_store_chunk(&self, chunks: Vec<(&[u8], &str, u32)>) -> reqwest::Result<serde_json::Value> {
//         let chunks: Vec<serde_json::Value> = chunks.iter().map(|(data, namespace, index)| {
//             json!({
//                 "data": BASE64.encode(data),
//                 "namespace": namespace,
//                 "chunk_index": index,
//             })
//         }).collect();

//         let response = self.client
//             .post(&format!("{}/batch-store", self.base_url))
//             .json(&json!({
//                 "chunks": chunks,
//             }))
//             .send()
//             .await?;

//         assert_eq!(response.status(), StatusCode::OK);
//         response.json().await
//     }

//     async fn store_chunk(&self, data: &[u8], namespace: &str, index: u32) -> reqwest::Result<serde_json::Value> {
//         let response = self.client
//             .post(&format!("{}/store", self.base_url))
//             .json(&json!({
//                 "data": BASE64.encode(data),
//                 "namespace": namespace,
//                 "chunk_index": index,
//             }))
//             .send()
//             .await?;

//         assert_eq!(response.status(), StatusCode::OK);
//         response.json().await
//     }

//     async fn retrieve_chunk(&self, chunk_id: &str) -> reqwest::Result<serde_json::Value> {
//         let response = self.client
//             .get(&format!("{}/retrieve/{}", self.base_url, chunk_id))
//             .send()
//             .await?;

//         response.json().await
//     }

//     async fn check_status(&self, chunk_id: &str) -> reqwest::Result<serde_json::Value> {
//         let response = self.client
//             .get(&format!("{}/status/{}", self.base_url, chunk_id))
//             .send()
//             .await?;

//         response.json().await
//     }

//     async fn delete_chunk(&self, chunk_id: &str) -> reqwest::Result<serde_json::Value> {
//         let response = self.client
//             .delete(&format!("{}/delete/{}", self.base_url, chunk_id))
//             .send()
//             .await?;

//         response.json().await
//     }

//     async fn list_chunks(&self, offset: Option<usize>, limit: Option<usize>) -> reqwest::Result<serde_json::Value> {
//         let mut url = format!("{}/list", self.base_url);
//         if let Some(offset) = offset {
//             url.push_str(&format!("?offset={}", offset));
//         }
//         if let Some(limit) = limit {
//             url.push_str(&format!("{}limit={}", if url.contains('?') { "&" } else { "?" }, limit));
//         }

//         let response = self.client
//             .get(&url)
//             .send()
//             .await?;

//         response.json().await
//     }

//     async fn batch_retrieve_chunk(&self, chunk_ids: Vec<String>) -> reqwest::Result<serde_json::Value> {
//         let response = self.client
//             .post(&format!("{}/batch-retrieve", self.base_url))
//             .json(&json!({
//                 "chunk_ids": chunk_ids,
//             }))
//             .send()
//             .await?;

//         response.json().await
//     }
// }


// async fn setup_actors() -> Vec<PrivateKeySigner> {
//     let mut actors = Vec::new();
//     for i in 0..10 {
//         let signer = PrivateKeySigner::random();
//         actors.push(signer);
//     }
//     actors
// }


// #[tokio::test]
// async fn test_batch_store_and_batch_retrieve() {
//     let server = TestServer::new().await;
    
//     // Generate test data with different namespaces and indices
//     let mut testdata = Vec::new();
//     for i in 0..5 {
//         let random_bytes: [u8; 100] = rand::rng().random();
//         let namespace = format!("batch-test-{}", i % 2); // Two different namespaces
//         let index = i as u32;
//         testdata.push((random_bytes, namespace, index));
//     }

//     // Convert testdata to the format expected by batch_store_chunk
//     let chunks: Vec<(&[u8], &str, u32)> = testdata.iter().map(|(data, namespace, index)| {
//         (data.as_slice(), namespace.as_str(), *index)
//     }).collect();

//     // Store chunks in batch
//     let batch_response = server.batch_store_chunk(chunks).await.unwrap();
//     let results = batch_response["results"].as_array().unwrap();
//     assert_eq!(results.len(), 5);

//     // Verify all chunks were stored successfully and collect chunk IDs
//     let mut chunk_ids = Vec::new();
//     for (i, result) in results.iter().enumerate() {
//         assert!(result["success"].as_bool().unwrap(), "Chunk {} should be stored successfully", i);
//         let chunk_id = result["chunk_id"].as_str().unwrap();
//         let hash = result["hash"].as_str().unwrap();
//         chunk_ids.push(chunk_id.to_string());
        
//         // Verify individual retrieval works
//         let retrieve_response = server.retrieve_chunk(chunk_id).await.unwrap();
//         assert_eq!(retrieve_response["chunk_id"], chunk_id);
//         assert_eq!(retrieve_response["metadata"]["hash"].as_str().unwrap().trim_start_matches("0x"), hash);
        
//         // Verify the data matches
//         let retrieved_data = BASE64.decode(retrieve_response["data"].as_str().unwrap()).unwrap();
//         assert_eq!(retrieved_data, testdata[i].0);
//     }

//     // Test batch retrieve with all valid chunk IDs
//     let batch_retrieve_response = server.batch_retrieve_chunk(chunk_ids.clone()).await.unwrap();
//     let retrieved_chunks = batch_retrieve_response["chunks"].as_array().unwrap();
//     let errors = batch_retrieve_response["errors"].as_array().unwrap();
    
//     assert_eq!(retrieved_chunks.len(), 5);
//     assert_eq!(errors.len(), 0);

//     // Verify all retrieved chunks have correct data
//     for (i, chunk) in retrieved_chunks.iter().enumerate() {
//         let retrieved_data = BASE64.decode(chunk["data"].as_str().unwrap()).unwrap();
//         assert_eq!(retrieved_data, testdata[i].0);
//         assert_eq!(chunk["metadata"]["namespace"], testdata[i].1);
//         assert_eq!(chunk["metadata"]["index"], testdata[i].2);
//     }

//     // Test batch retrieve with some invalid chunk IDs
//     let mut mixed_chunk_ids = chunk_ids.clone();
//     mixed_chunk_ids.push("non-existent-chunk-1".to_string());
//     mixed_chunk_ids.push("non-existent-chunk-2".to_string());

//     let batch_retrieve_response = server.batch_retrieve_chunk(mixed_chunk_ids).await.unwrap();
//     let retrieved_chunks = batch_retrieve_response["chunks"].as_array().unwrap();
//     let errors = batch_retrieve_response["errors"].as_array().unwrap();
    
//     assert_eq!(retrieved_chunks.len(), 5); // Only valid chunks should be retrieved
//     assert_eq!(errors.len(), 2); // Two errors for non-existent chunks

//     // Verify error messages
//     for error in errors {
//         let error_msg = error.as_str().unwrap();
//         assert!(error_msg.contains("not found"));
//     }

//     // Test batch retrieve with all invalid chunk IDs
//     let invalid_chunk_ids = vec![
//         "invalid-chunk-1".to_string(),
//         "invalid-chunk-2".to_string(),
//         "invalid-chunk-3".to_string(),
//     ];

//     let batch_retrieve_response = server.batch_retrieve_chunk(invalid_chunk_ids).await.unwrap();
//     let retrieved_chunks = batch_retrieve_response["chunks"].as_array().unwrap();
//     let errors = batch_retrieve_response["errors"].as_array().unwrap();
    
//     assert_eq!(retrieved_chunks.len(), 0);
//     assert_eq!(errors.len(), 3);

//     // Test batch retrieve with empty list
//     let empty_response = server.batch_retrieve_chunk(vec![]).await.unwrap();
//     let empty_chunks = empty_response["chunks"].as_array().unwrap();
//     let empty_errors = empty_response["errors"].as_array().unwrap();
    
//     assert_eq!(empty_chunks.len(), 0);
//     assert_eq!(empty_errors.len(), 0);
// }

// #[tokio::test]
// async fn test_store_and_retrieve() {
//     let server = TestServer::new().await;
//     // random string
//     let random_bytes: [u8; 100] = rand::rng().random();
//     let namespace = "test-namespace";
//     let index = 1;

//     // Store a chunk
//     let store_response = server.store_chunk(&random_bytes, namespace, index).await.unwrap();
//     assert!(store_response["success"].as_bool().unwrap());
//     let chunk_id = store_response["chunk_id"].as_str().unwrap();
//     let hash = store_response["hash"].as_str().unwrap();

//     // Verify status
//     let status = server.check_status(chunk_id).await.unwrap();
//     assert!(status["exists"].as_bool().unwrap());
//     let metadata = status["metadata"].as_object().unwrap();
//     assert_eq!(metadata["namespace"], namespace);
//     assert_eq!(metadata["index"], index);
//     let metadata_hash = metadata["hash"].as_str().unwrap().trim_start_matches("0x");
//     assert_eq!(metadata_hash, hash);

//     // Retrieve the chunk
//     let retrieve_response = server.retrieve_chunk(chunk_id).await.unwrap();
//     let retrieved_data = BASE64.decode(retrieve_response["data"].as_str().unwrap()).unwrap();
//     assert_eq!(retrieved_data, &random_bytes);
//     assert_eq!(retrieve_response["chunk_id"], chunk_id);
//     assert_eq!(retrieve_response["metadata"]["namespace"], namespace);
//     assert_eq!(retrieve_response["metadata"]["index"], index);
//     let retrieved_hash = retrieve_response["metadata"]["hash"].as_str().unwrap().trim_start_matches("0x");
//     assert_eq!(retrieved_hash, hash);
// }

// #[tokio::test]
// async fn test_delete() {
//     let server = TestServer::new().await;
//     let random_bytes: [u8; 100] = rand::rng().random();
//     let namespace = "delete-test";
//     let index = 1;

//     // Store a chunk
//     let store_response = server.store_chunk(&random_bytes, namespace, index).await.unwrap();
//     let chunk_id = store_response["chunk_id"].as_str().unwrap();

//     // Verify it exists
//     let status = server.check_status(chunk_id).await.unwrap();
//     assert!(status["exists"].as_bool().unwrap());

//     // Delete the chunk
//     let delete_response = server.delete_chunk(chunk_id).await.unwrap();
//     assert!(delete_response["deleted"].as_bool().unwrap());

//     // Verify it's gone
//     let status = server.check_status(chunk_id).await.unwrap();
//     assert!(!status["exists"].as_bool().unwrap());

//     // Try to delete non-existent chunk
//     let delete_response = server.delete_chunk("non-existent").await.unwrap();
//     assert!(!delete_response["deleted"].as_bool().unwrap());
// }

// #[tokio::test]
// async fn test_list_chunks() {
//     println!("lalala");
//     let server = TestServer::new().await;
//     let test_data = b"Test data";
//     let namespace = "list-test";

//     // Store multiple chunks with predictable IDs
//     for i in 0..5 {
//         let hash_32 = FixedBytes::<32>::new([i as u8; 32]);
//         let chunk_id = format!("{}_{}", namespace, format!("{:016x}", i));
        
//         // Store directly using the storage API
//         let storage = FileStorage::new(server._temp_dir.path());
//         // storage.store(&chunk_id, test_data, metadata).await.unwrap();
//     }

//     // Test pagination
//     let list_response = server.list_chunks(Some(0), Some(2)).await.unwrap();
//     println!("{:?}", list_response);
//     let chunks = list_response["chunks"].as_array().unwrap();
//     assert_eq!(chunks.len(), 2);

//     // Test offset
//     let list_response = server.list_chunks(Some(2), Some(2)).await.unwrap();
//     let chunks = list_response["chunks"].as_array().unwrap();
//     assert_eq!(chunks.len(), 2);

//     // Test limit larger than available
//     let list_response = server.list_chunks(Some(4), Some(10)).await.unwrap();
//     let chunks = list_response["chunks"].as_array().unwrap();
//     assert_eq!(chunks.len(), 1);
// }

// #[tokio::test]
// async fn test_error_cases() {
//     let server = TestServer::new().await;

//     // Test retrieving non-existent chunk
//     let response = server.retrieve_chunk("non-existent").await.unwrap();
//     assert!(response["error"].as_str().unwrap().contains("not found"));

//     // Test storing invalid base64 data
//     let response = server.client
//         .post(&format!("{}/store", server.base_url))
//         .json(&json!({
//             "data": "invalid-base64!",
//             "namespace": "test",
//             "chunk_index": 1,
//         }))
//         .send()
//         .await
//         .unwrap();

//     assert_eq!(response.status(), StatusCode::BAD_REQUEST);
// }

// #[tokio::test]
// async fn test_batch_store_error_cases() {
//     let server = TestServer::new().await;

//     // Create a custom request with invalid base64
//     let response = server.client
//         .post(&format!("{}/batch-store", server.base_url))
//         .json(&json!({
//             "chunks": [
//                 {
//                     "data": "invalid-base64!",
//                     "namespace": "test-namespace",
//                     "chunk_index": 1
//                 },
//                 {
//                     "data": BASE64.encode(b"valid data"),
//                     "namespace": "test-namespace", 
//                     "chunk_index": 2
//                 }
//             ]
//         }))
//         .send()
//         .await
//         .unwrap();

//     assert_eq!(response.status(), StatusCode::OK);
//     let batch_response = response.json::<serde_json::Value>().await.unwrap();
//     let results = batch_response["results"].as_array().unwrap();
    
//     // First chunk should fail due to invalid base64
//     assert!(!results[0]["success"].as_bool().unwrap());
//     assert_eq!(results[0]["chunk_id"], "");
//     assert_eq!(results[0]["hash"], "");
    
//     // Second chunk should succeed
//     assert!(results[1]["success"].as_bool().unwrap());
//     assert!(!results[1]["chunk_id"].as_str().unwrap().is_empty());
//     assert!(!results[1]["hash"].as_str().unwrap().is_empty());
// } 

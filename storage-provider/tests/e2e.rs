use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::time::sleep;
use reqwest::StatusCode;
use serde_json::json;
use storage_provider::{
    file_storage::FileStorage,
    http::start_server,
    ChunkStorage,
    ChunkMetadata,
};
use tempfile::TempDir;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use tokio::sync::oneshot;
use std::net::TcpListener;

struct TestServer {
    _temp_dir: TempDir,
    client: reqwest::Client,
    base_url: String,
    _shutdown: oneshot::Sender<()>,
}

impl TestServer {
    async fn new() -> Self {
        // Find an available port
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener); // Close the listener so the port is free

        // Create a temporary directory for storage
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::new(temp_dir.path());
        let storage = Arc::new(storage);

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        // Start the server in the background
        let server = start_server(storage, port);
        let _ = tokio::spawn(async move {
            let server = server;
            tokio::select! {
                _ = server => {},
                _ = shutdown_rx => {},
            }
        });

        // Wait for server to start
        sleep(Duration::from_millis(100)).await;

        let client = reqwest::Client::new();
        let base_url = format!("http://localhost:{}", port);

        Self {
            _temp_dir: temp_dir,
            client,
            base_url,
            _shutdown: shutdown_tx,
        }
    }

    async fn store_chunk(&self, data: &[u8], namespace: &str, index: u32) -> reqwest::Result<serde_json::Value> {
        let response = self.client
            .post(&format!("{}/store", self.base_url))
            .json(&json!({
                "data": BASE64.encode(data),
                "namespace": namespace,
                "chunk_index": index,
            }))
            .send()
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        response.json().await
    }

    async fn retrieve_chunk(&self, chunk_id: &str) -> reqwest::Result<serde_json::Value> {
        let response = self.client
            .get(&format!("{}/retrieve/{}", self.base_url, chunk_id))
            .send()
            .await?;

        response.json().await
    }

    async fn check_status(&self, chunk_id: &str) -> reqwest::Result<serde_json::Value> {
        let response = self.client
            .get(&format!("{}/status/{}", self.base_url, chunk_id))
            .send()
            .await?;

        response.json().await
    }

    async fn delete_chunk(&self, chunk_id: &str) -> reqwest::Result<serde_json::Value> {
        let response = self.client
            .delete(&format!("{}/delete/{}", self.base_url, chunk_id))
            .send()
            .await?;

        response.json().await
    }

    async fn list_chunks(&self, offset: Option<usize>, limit: Option<usize>) -> reqwest::Result<serde_json::Value> {
        let mut url = format!("{}/list", self.base_url);
        if let Some(offset) = offset {
            url.push_str(&format!("?offset={}", offset));
        }
        if let Some(limit) = limit {
            url.push_str(&format!("{}limit={}", if url.contains('?') { "&" } else { "?" }, limit));
        }

        let response = self.client
            .get(&url)
            .send()
            .await?;

        response.json().await
    }
}

#[tokio::test]
async fn test_store_and_retrieve() {
    let server = TestServer::new().await;
    let test_data = b"Hello, World!";
    let namespace = "test-namespace";
    let index = 1;

    // Store a chunk
    let store_response = server.store_chunk(test_data, namespace, index).await.unwrap();
    assert!(store_response["success"].as_bool().unwrap());
    let chunk_id = store_response["chunk_id"].as_str().unwrap();
    let hash = store_response["hash"].as_str().unwrap();

    // Verify status
    let status = server.check_status(chunk_id).await.unwrap();
    assert!(status["exists"].as_bool().unwrap());
    let metadata = status["metadata"].as_object().unwrap();
    assert_eq!(metadata["namespace"], namespace);
    assert_eq!(metadata["index"], index);
    assert_eq!(metadata["hash"], hash);

    // Retrieve the chunk
    let retrieve_response = server.retrieve_chunk(chunk_id).await.unwrap();
    let retrieved_data = BASE64.decode(retrieve_response["data"].as_str().unwrap()).unwrap();
    assert_eq!(retrieved_data, test_data);
    assert_eq!(retrieve_response["chunk_id"], chunk_id);
    assert_eq!(retrieve_response["metadata"]["namespace"], namespace);
    assert_eq!(retrieve_response["metadata"]["index"], index);
    assert_eq!(retrieve_response["metadata"]["hash"], hash);
}

#[tokio::test]
async fn test_delete() {
    let server = TestServer::new().await;
    let test_data = b"Test data for deletion";
    let namespace = "delete-test";
    let index = 1;

    // Store a chunk
    let store_response = server.store_chunk(test_data, namespace, index).await.unwrap();
    let chunk_id = store_response["chunk_id"].as_str().unwrap();

    // Verify it exists
    let status = server.check_status(chunk_id).await.unwrap();
    assert!(status["exists"].as_bool().unwrap());

    // Delete the chunk
    let delete_response = server.delete_chunk(chunk_id).await.unwrap();
    assert!(delete_response["deleted"].as_bool().unwrap());

    // Verify it's gone
    let status = server.check_status(chunk_id).await.unwrap();
    assert!(!status["exists"].as_bool().unwrap());

    // Try to delete non-existent chunk
    let delete_response = server.delete_chunk("non-existent").await.unwrap();
    assert!(!delete_response["deleted"].as_bool().unwrap());
}

#[tokio::test]
async fn test_list_chunks() {
    let server = TestServer::new().await;
    let test_data = b"Test data";
    let namespace = "list-test";
    let base_hash = "test-hash";

    // Store multiple chunks with predictable IDs
    for i in 0..5 {
        let chunk_id = format!("{}_{}", namespace, format!("{:016x}", i));
        let metadata = ChunkMetadata {
            namespace: namespace.to_string(),
            index: i,
            hash: format!("{}-{}", base_hash, i),
            stored_at: SystemTime::now(),
        };
        
        // Store directly using the storage API
        let storage = FileStorage::new(server._temp_dir.path());
        storage.store(&chunk_id, test_data, metadata).await.unwrap();
    }

    // Test pagination
    let list_response = server.list_chunks(Some(0), Some(2)).await.unwrap();
    let chunks = list_response["chunks"].as_array().unwrap();
    assert_eq!(chunks.len(), 2);

    // Test offset
    let list_response = server.list_chunks(Some(2), Some(2)).await.unwrap();
    let chunks = list_response["chunks"].as_array().unwrap();
    assert_eq!(chunks.len(), 2);

    // Test limit larger than available
    let list_response = server.list_chunks(Some(4), Some(10)).await.unwrap();
    let chunks = list_response["chunks"].as_array().unwrap();
    assert_eq!(chunks.len(), 1);
}

#[tokio::test]
async fn test_error_cases() {
    let server = TestServer::new().await;

    // Test retrieving non-existent chunk
    let response = server.retrieve_chunk("non-existent").await.unwrap();
    assert!(response["error"].as_str().unwrap().contains("not found"));

    // Test storing invalid base64 data
    let response = server.client
        .post(&format!("{}/store", server.base_url))
        .json(&json!({
            "data": "invalid-base64!",
            "namespace": "test",
            "chunk_index": 1,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
} 
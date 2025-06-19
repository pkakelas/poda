use std::convert::Infallible;
use std::sync::Arc;
use alloy::primitives::FixedBytes;
use kzg::{kzg_multi_verify, kzg_verify};
use warp::Filter;
use serde::{Deserialize, Serialize};
use pod::client::{PodaClient, PodaClientTrait};
use crate::storage::ChunkStorage;
use types::{Chunk, KzgProof};
use hex;

#[derive(Debug, Deserialize)]
struct StoreRequest {
    commitment: FixedBytes<32>,
    namespace: String,
    chunk: Chunk,
    kzg_proof: KzgProof,
}

#[derive(Debug, Serialize)]
struct StoreResponse {
    success: bool,
    message: String,
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    exists: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BatchStoreRequest {
    pub commitment: FixedBytes<32>,
    pub namespace: String,
    pub chunks: Vec<Chunk>,
    pub kzg_proof: KzgProof,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchRetrieveRequest {
    pub namespace: String,
    pub commitment: FixedBytes<32>,
    pub indices: Vec<u16>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchRetrieveResponse {
    pub chunks: Vec<Option<Chunk>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchDeleteRequest {
    pub namespace: String,
    pub commitment: FixedBytes<32>,
    pub indices: Vec<u16>,
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    namespace: String,
    commitment: String,
}

#[derive(Debug, Serialize)]
struct ListResponse {
    indices: Vec<u16>,
}


pub async fn start_server<T: ChunkStorage + Send + Sync + 'static>(
    storage: Arc<T>,
    pod: Arc<PodaClient>,
    port: u16,
) {
    let storage_filter = warp::any().map(move || storage.clone());
    let pod_filter = warp::any().map(move || pod.clone());


    // POST /store - Store a new chunk
    let store = warp::path("store")
        .and(warp::post())
        .and(warp::body::json())
        .and(storage_filter.clone())
        .and(pod_filter.clone())
        .and_then(handle_store);

    // POST /batch-store - Store multiple chunks
    let batch_store = warp::path("batch-store")
        .and(warp::post())
        .and(warp::body::json())
        .and(storage_filter.clone())
        .and(pod_filter.clone())
        .and_then(handle_batch_store);

    // GET /retrieve/{chunk_id} - Retrieve a chunk
    let retrieve = warp::path!("retrieve" / String)
        .and(warp::get())
        .and(storage_filter.clone())
        .and(pod_filter.clone())
        .and_then(handle_retrieve);

    // POST /batch-retrieve - Retrieve multiple chunks
    let batch_retrieve = warp::path("batch-retrieve")
        .and(warp::post())
        .and(warp::body::json())
        .and(storage_filter.clone())
        .and(pod_filter.clone())
        .and_then(handle_batch_retrieve);

    // GET /status/{chunk_id} - Check if chunk exists
    let status = warp::path!("status" / String)
        .and(warp::get())
        .and(storage_filter.clone())
        .and(pod_filter.clone())
        .and_then(handle_status);

    // DELETE /delete/{chunk_id} - Delete a chunk
    let delete = warp::path!("delete")
        .and(warp::post())
        .and(warp::body::json())
        .and(storage_filter.clone())
        .and(pod_filter.clone())
        .and_then(handle_batch_delete);

    // GET /list?offset=0&limit=10 - List chunks
    let list = warp::path("list")
        .and(warp::get())
        .and(warp::query::<ListQuery>())
        .and(storage_filter.clone())
        .and(pod_filter.clone())
        .and_then(handle_list);

    // GET /health - Health check
    let health_check = warp::path("health")
        .and(warp::get())
        .and_then(handle_health_check);

    let routes = store
        .or(batch_store)
        .or(retrieve)
        .or(batch_retrieve)
        .or(status)
        .or(delete)
        .or(list)
        .or(health_check)
        .with(warp::cors().allow_any_origin());


    println!("🦀 Rust Storage Provider API starting on port {}", port);
    warp::serve(routes).run(([127, 0, 0, 1], port)).await;
}

async fn handle_health_check() -> Result<impl warp::Reply, Infallible> {
    Ok(warp::reply::with_status(
        warp::reply::json(&serde_json::json!({"status": "ok"})),
        warp::http::StatusCode::OK,
    ))
}

async fn handle_store<T: ChunkStorage>(
    request: StoreRequest,
    storage: Arc<T>,
    pod: Arc<PodaClient>,
) -> Result<impl warp::Reply, Infallible> {
    let commitment = pod.get_commitment_info(request.commitment).await;
    if commitment.is_err() {
        return Ok(warp::reply::with_status(
            warp::reply::json(&StoreResponse {
                success: false,
                message: format!("Failed to get commitment info: {:?}", commitment.err()),
            }),
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        ));
    }

    let (commitment_info, _) = commitment.unwrap();
    let is_valid = kzg_verify(&request.chunk, request.chunk.index as usize, commitment_info.kzgCommitment.try_into().unwrap(), request.kzg_proof);
    if !is_valid {
        return Ok(warp::reply::with_status(
            warp::reply::json(&StoreResponse {
                success: false,
                message: "KZG proof verification failed".to_string(),
            }),
            warp::http::StatusCode::BAD_REQUEST,
        ));
    }

    match storage.store(request.namespace, request.commitment, &request.chunk).await {
        Ok(_) => {
            println!("Chunk stored successfully");

            let res = pod.submit_chunk_attestations(request.commitment, vec![request.chunk.index]).await;
            if res.is_err() {
                return Ok(warp::reply::with_status(
                    warp::reply::json(&StoreResponse {
                        success: false,
                        message: format!("Failed to submit chunk attestation: {:?}", res.err()),
                    }),
                    warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                ));
            }

            if !is_valid {
                return Ok(warp::reply::with_status(
                    warp::reply::json(&StoreResponse {
                        success: false,
                        message: "KZG proof verification failed".to_string(),
                    }),
                    warp::http::StatusCode::INTERNAL_SERVER_ERROR,  
                ));
            }

            Ok(warp::reply::with_status(
                warp::reply::json(&StoreResponse {
                    success: true,
                    message: "Chunk stored successfully".to_string(),
                }),
                warp::http::StatusCode::OK,
            ))
        }

        Err(e) => {
            println!("Error storing chunk: {:?}", e);
            Ok(warp::reply::with_status(
                warp::reply::json(&StoreResponse {
                    success: false,
                    message: format!("Failed to store chunk: {:?}", e),
                }),
                warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            ))
        }
    }
}

async fn handle_batch_retrieve<T: ChunkStorage>(
    request: BatchRetrieveRequest,
    storage: Arc<T>,
    _: Arc<PodaClient>,
) -> Result<impl warp::Reply, Infallible> {
    println!("Retrieving chunks: {:?}", request);
    let mut chunks = Vec::new();
    let mut errors = Vec::new();

    for index in &request.indices {
        match storage.retrieve(request.namespace.clone(), request.commitment, *index).await {
            Ok(Some(chunk)) => {
                chunks.push(Some(chunk));
            }
            Ok(None) => {
                errors.push(format!("Chunk not found at index: {}", index));
                chunks.push(None);
            }
            Err(_) => {
                errors.push(format!("Failed to retrieve chunk at index: {}", index));
                chunks.push(None);
            }
        }
    }

    let none_chunks = chunks.iter().filter(|c| c.is_none()).count();
    if none_chunks == request.indices.len() {
        return Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({"error": "All chunks not found"})),
            warp::http::StatusCode::NOT_FOUND,
        ));
    }

    Ok(warp::reply::with_status(
        warp::reply::json(&BatchRetrieveResponse { chunks: chunks }),
        warp::http::StatusCode::OK,
    ))
}

async fn handle_retrieve<T: ChunkStorage>(
    chunk_id: String,
    storage: Arc<T>,
    _: Arc<PodaClient>,
) -> Result<impl warp::Reply, Infallible> {
    // Parse chunk_id to extract namespace, commitment, and index
    // Format: {namespace}_{commitment}_{index}
    let parts: Vec<&str> = chunk_id.split('_').collect();
    if parts.len() < 3 {
        return Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({"error": "Invalid chunk ID format"})),
            warp::http::StatusCode::BAD_REQUEST,
        ));
    }

    let namespace = parts[0].to_string();
    let commitment_hex = parts[1];
    let index_str = parts[2];

    let commitment = match hex::decode(commitment_hex) {
        Ok(bytes) if bytes.len() == 32 => FixedBytes::from_slice(&bytes),
        _ => {
            return Ok(warp::reply::with_status(
                warp::reply::json(&serde_json::json!({"error": "Invalid commitment format"})),
                warp::http::StatusCode::BAD_REQUEST,
            ));
        }
    };

    let index = match index_str.parse::<u16>() {
        Ok(idx) => idx,
        Err(_) => {
            return Ok(warp::reply::with_status(
                warp::reply::json(&serde_json::json!({"error": "Invalid index format"})),
                warp::http::StatusCode::BAD_REQUEST,
            ));
        }
    };

    match storage.retrieve(namespace, commitment, index).await {
        Ok(Some(chunk)) => {
            Ok(warp::reply::with_status(
                warp::reply::json(&Some(chunk)),
                warp::http::StatusCode::OK,
            ))
        }
        Ok(None) => Ok(warp::reply::with_status(
            warp::reply::json(&None::<Chunk>),
            warp::http::StatusCode::NOT_FOUND,
        )),
        Err(_) => Ok(warp::reply::with_status(
            warp::reply::json(&None::<Chunk>),
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        )),
    }
}

async fn handle_status<T: ChunkStorage>(
    chunk_id: String,
    storage: Arc<T>,
    _: Arc<PodaClient>,
) -> Result<impl warp::Reply, Infallible> {
    // Parse chunk_id to extract namespace, commitment, and index
    let parts: Vec<&str> = chunk_id.split('_').collect();
    if parts.len() < 3 {
        return Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({"error": "Invalid chunk ID format"})),
            warp::http::StatusCode::BAD_REQUEST,
        ));
    }

    let namespace = parts[0].to_string();
    let commitment_hex = parts[1];
    let index_str = parts[2];

    let commitment = match hex::decode(commitment_hex) {
        Ok(bytes) if bytes.len() == 32 => FixedBytes::from_slice(&bytes),
        _ => {
            return Ok(warp::reply::with_status(
                warp::reply::json(&serde_json::json!({"error": "Invalid commitment format"})),
                warp::http::StatusCode::BAD_REQUEST,
            ));
        }
    };

    let index = match index_str.parse::<u16>() {
        Ok(idx) => idx,
        Err(_) => {
            return Ok(warp::reply::with_status(
                warp::reply::json(&serde_json::json!({"error": "Invalid index format"})),
                warp::http::StatusCode::BAD_REQUEST,
            ));
        }
    };

    match storage.exists(namespace, commitment, index).await {
        Ok(exists) => {
            Ok(warp::reply::with_status(
                warp::reply::json(&StatusResponse { exists }),
                warp::http::StatusCode::OK,
            ))
        }
        Err(_) => Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({"error": "Internal server error"})),
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        )),
    }
}

async fn handle_batch_delete<T: ChunkStorage>(
    request: BatchDeleteRequest,
    storage: Arc<T>,
    _: Arc<PodaClient>,
) -> Result<impl warp::Reply, Infallible> {
    for index in request.indices {
        match storage.delete(request.namespace.clone(), request.commitment, index).await {
            Ok(_) => {},
            Err(_) => {
                return Ok(warp::reply::with_status(
                    warp::reply::json(&serde_json::json!({"error": "Internal server error"})),
                    warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                ))
            }
        }
    }

    Ok(warp::reply::with_status(warp::reply::json(&serde_json::json!({"success": true})), warp::http::StatusCode::OK))
}

async fn handle_batch_store<T: ChunkStorage>(
    request: BatchStoreRequest,
    storage: Arc<T>,
    pod: Arc<PodaClient>,
) -> Result<impl warp::Reply, Infallible> {
    let commitment = pod.get_commitment_info(request.commitment).await;
    if commitment.is_err() {
        let err = commitment.err();

        println!("[STORAGE_PROVIDER] Failed to get commitment info: {:?}", err);
        return Ok(warp::reply::with_status(
            warp::reply::json(&StoreResponse {
                success: false,
                message: format!("Failed to get commitment info: {:?}", err),
            }),
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        ));
    }

    let (commitment_info, _) = commitment.unwrap();
    println!("[STORAGE_PROVIDER] Got commitment info: {:?}", commitment_info);
    let chunk_indices = request.chunks.iter().map(|c| c.index as usize).collect::<Vec<_>>();
    println!("[STORAGE_PROVIDER] Verifying KZG proof for chunks: {:?}", chunk_indices);
    let is_valid = kzg_multi_verify(&request.chunks, chunk_indices.as_slice(), commitment_info.kzgCommitment.try_into().unwrap(), request.kzg_proof);
    println!("[STORAGE_PROVIDER] KZG proof verification result: {:?}", is_valid);

    if !is_valid {
        return Ok(warp::reply::with_status(
            warp::reply::json(&StoreResponse {
                success: false,
                message: "KZG proof verification failed".to_string(),
            }),
            warp::http::StatusCode::BAD_REQUEST,
        ));
    }

    for chunk in &request.chunks {
        match storage.store(request.namespace.clone(), request.commitment, &chunk).await {
            Ok(_) => {
            }
            Err(e) => {
                return Ok(warp::reply::with_status(
                    warp::reply::json(&StoreResponse {
                        success: false,
                        message: format!("Failed to store chunk: {:?}", e),
                    }),
                    warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                ));
            }
        }
    }

    let indices = request.chunks.iter().map(|c| c.index as u16).collect::<Vec<_>>();
    println!("Submitting chunk attestation for indices: {:?}", indices);
    let res = pod.submit_chunk_attestations(request.commitment, indices).await;
    if res.is_err() {
        return Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({"error": "Failed to submit chunk attestation"})),
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        ));
    }

    Ok(warp::reply::with_status(warp::reply::json(&serde_json::json!({"success": true})), warp::http::StatusCode::OK))
}

async fn handle_list<T: ChunkStorage>(
    query: ListQuery,
    storage: Arc<T>,
    _: Arc<PodaClient>,
) -> Result<impl warp::Reply, Infallible> {
    // Parse commitment from string to FixedBytes
    let commitment = match hex::decode(&query.commitment) {
        Ok(bytes) if bytes.len() == 32 => FixedBytes::from_slice(&bytes),
        _ => {
            return Ok(warp::reply::with_status(
                warp::reply::json(&serde_json::json!({"error": "Invalid commitment format"})),
                warp::http::StatusCode::BAD_REQUEST,
            ));
        }
    };

    match storage.list_chunks(query.namespace, commitment).await {
        Ok(indices) => Ok(warp::reply::with_status(
            warp::reply::json(&ListResponse { indices }),
            warp::http::StatusCode::OK,
        )),
        Err(_) => Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({"error": "Internal server error"})),
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        )),
    }
}
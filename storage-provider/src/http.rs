use std::convert::Infallible;
use std::sync::Arc;
use base64::Engine;
use warp::Filter;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use crate::utils::base64_engine;
use crate::storage::{ChunkStorage, ChunkMetadata};
use std::time::SystemTime;

#[derive(Debug, Deserialize)]
struct StoreRequest {
    data: String, // base64 encoded data
    namespace: String,
    chunk_index: u32,
}

#[derive(Debug, Serialize)]
struct StoreResponse {
    chunk_id: String,
    hash: String,
    success: bool,
}

#[derive(Debug, Serialize)]
struct ChunkResponse {
    chunk_id: String,
    data: String, // base64 encoded
    metadata: ChunkMetadata,
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    exists: bool,
    metadata: Option<ChunkMetadata>,
}

pub async fn start_server<T: ChunkStorage + Send + Sync + 'static>(
    storage: Arc<T>,
    port: u16,
) {
    let storage_filter = warp::any().map(move || storage.clone());

    // POST /store - Store a new chunk
    let store = warp::path("store")
        .and(warp::post())
        .and(warp::body::json())
        .and(storage_filter.clone())
        .and_then(handle_store);

    // GET /retrieve/{chunk_id} - Retrieve a chunk
    let retrieve = warp::path!("retrieve" / String)
        .and(warp::get())
        .and(storage_filter.clone())
        .and_then(handle_retrieve);

    // GET /status/{chunk_id} - Check if chunk exists
    let status = warp::path!("status" / String)
        .and(warp::get())
        .and(storage_filter.clone())
        .and_then(handle_status);

    // DELETE /delete/{chunk_id} - Delete a chunk
    let delete = warp::path!("delete" / String)
        .and(warp::delete())
        .and(storage_filter.clone())
        .and_then(handle_delete);

    // GET /list?offset=0&limit=10 - List chunks
    let list = warp::path("list")
        .and(warp::get())
        .and(warp::query::<ListQuery>())
        .and(storage_filter.clone())
        .and_then(handle_list);

    let routes = store
        .or(retrieve)
        .or(status)
        .or(delete)
        .or(list)
        .with(warp::cors().allow_any_origin());

    println!("🦀 Rust Storage Provider API starting on port {}", port);
    warp::serve(routes).run(([127, 0, 0, 1], port)).await;
}

async fn handle_store<T: ChunkStorage>(
    request: StoreRequest,
    storage: Arc<T>,
) -> Result<impl warp::Reply, Infallible> {
    let data = match base64_engine().decode(&request.data) {
        Ok(data) => data,
        Err(_) => {
            return Ok(warp::reply::with_status(
                warp::reply::json(&StoreResponse {
                    chunk_id: String::new(),
                    hash: String::new(),
                    success: false,
                }),
                warp::http::StatusCode::BAD_REQUEST,
            ));
        }
    };

    // Generate chunk ID and hash
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash = hex::encode(hasher.finalize());
    let chunk_id = format!("{}_{}", request.namespace, hash[..16].to_string());

    let metadata = ChunkMetadata {
        namespace: request.namespace,
        index: request.chunk_index,
        hash: hash.clone(),
        stored_at: SystemTime::now(),
    };

    let success = storage.store(&chunk_id, &data, metadata).await.is_ok();

    Ok(warp::reply::with_status(
        warp::reply::json(&StoreResponse {
            chunk_id,
            hash,
            success,
        }),
        if success {
            warp::http::StatusCode::OK
        } else {
            warp::http::StatusCode::INTERNAL_SERVER_ERROR
        },
    ))
}

async fn handle_retrieve<T: ChunkStorage>(
    chunk_id: String,
    storage: Arc<T>,
) -> Result<impl warp::Reply, Infallible> {
    match storage.retrieve(&chunk_id).await {
        Ok(Some(chunk)) => {
            let data = base64_engine().encode(&chunk.data);
            Ok(warp::reply::with_status(
                warp::reply::json(&ChunkResponse {
                    chunk_id,
                    data,
                    metadata: chunk.metadata,
                }),
                warp::http::StatusCode::OK,
            ))
        }
        Ok(None) => Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({"error": "Chunk not found"})),
            warp::http::StatusCode::NOT_FOUND,
        )),
        Err(_) => Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({"error": "Internal server error"})),
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        )),
    }
}

async fn handle_status<T: ChunkStorage>(
    chunk_id: String,
    storage: Arc<T>,
) -> Result<impl warp::Reply, Infallible> {
    match storage.exists(&chunk_id).await {
        Ok(exists) => {
            let metadata = if exists {
                storage.retrieve(&chunk_id).await.ok().flatten().map(|c| c.metadata)
            } else {
                None
            };

            Ok(warp::reply::with_status(
                warp::reply::json(&StatusResponse { exists, metadata }),
                warp::http::StatusCode::OK,
            ))
        }
        Err(_) => Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({"error": "Internal server error"})),
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        )),
    }
}

async fn handle_delete<T: ChunkStorage>(
    chunk_id: String,
    storage: Arc<T>,
) -> Result<impl warp::Reply, Infallible> {
    match storage.delete(&chunk_id).await {
        Ok(deleted) => Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({"deleted": deleted})),
            warp::http::StatusCode::OK,
        )),
        Err(_) => Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({"error": "Internal server error"})),
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        )),
    }
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    offset: Option<usize>,
    limit: Option<usize>,
}

async fn handle_list<T: ChunkStorage>(
    query: ListQuery,
    storage: Arc<T>,
) -> Result<impl warp::Reply, Infallible> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(10);

    match storage.list_chunks(offset, limit).await {
        Ok(chunks) => Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({"chunks": chunks})),
            warp::http::StatusCode::OK,
        )),
        Err(_) => Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({"error": "Internal server error"})),
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        )),
    }
}
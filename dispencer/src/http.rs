use std::convert::Infallible;
use std::sync::Arc;
use pod::FixedBytes;
use warp::Filter;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use sha3::{Digest, Keccak256};
use crate::dispenser::Dispenser;
use pod::client::PodaClientTrait;

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitDataRequest {
    pub namespace: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitDataResponse {
    pub success: bool,
    pub message: String,
    pub commitment: FixedBytes<32>,
    pub assignments: std::collections::HashMap<String, Vec<u16>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RetrieveDataRequest {
    pub namespace: String,
    pub commitment: FixedBytes<32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RetrieveDataResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<Vec<u8>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    status: String,
}

pub async fn start_server<T: PodaClientTrait + Send + Sync + 'static>(
    dispenser: Dispenser<T>,
    port: u16,
) {
    let dispenser = Arc::new(dispenser);
    let dispenser_filter = warp::any().map(move || dispenser.clone());

    // POST /submit - Submit data for storage
    let submit = warp::path("submit")
        .and(warp::post())
        .and(warp::body::json())
        .and(dispenser_filter.clone())
        .and_then(handle_submit_data);

    // POST /retrieve - Retrieve data
    let retrieve = warp::path("retrieve")
        .and(warp::post())
        .and(warp::body::json())
        .and(dispenser_filter.clone())
        .and_then(handle_retrieve_data);

    // GET /health - Health check
    let health_check = warp::path("health")
        .and(warp::get())
        .and_then(handle_health_check);

    let routes = submit
        .or(retrieve)
        .or(health_check)
        .with(warp::cors().allow_any_origin());

    println!("🦀 Rust Dispenser API starting on port {}", port);
    warp::serve(routes).run(([127, 0, 0, 1], port)).await;
}

async fn handle_health_check() -> Result<impl warp::Reply, Infallible> {
    Ok(warp::reply::with_status(
        warp::reply::json(&HealthResponse {
            status: "ok".to_string(),
        }),
        warp::http::StatusCode::OK,
    ))
}

async fn handle_submit_data<T: PodaClientTrait>(
    request: SubmitDataRequest,
    dispenser: Arc<Dispenser<T>>,
) -> Result<impl warp::Reply, Infallible> {
    match dispenser.submit_data(request.namespace, &request.data).await {
        Ok(assignments) => {
            let commitment: FixedBytes<32> = FixedBytes::from_slice(&Keccak256::digest(&request.data));

            // Convert assignments to a simpler format for JSON serialization
            let mut assignments_json = std::collections::HashMap::new();
            for (provider_name, chunks) in assignments {
                let indices: Vec<u16> = chunks.iter().map(|c| c.index).collect();
                assignments_json.insert(provider_name, indices);
            }

            Ok(warp::reply::with_status(
                warp::reply::json(&SubmitDataResponse {
                    success: true,
                    message: "Data submitted successfully".to_string(),
                    commitment,
                    assignments: assignments_json,
                }),
                warp::http::StatusCode::OK,
            ))
        }
        Err(e) => {
            Ok(warp::reply::with_status(
                warp::reply::json(&SubmitDataResponse {
                    success: false,
                    message: format!("Failed to submit data: {:?}", e),
                    commitment: FixedBytes::default(),
                    assignments: std::collections::HashMap::new(),
                }),
                warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            ))
        }
    }
}

async fn handle_retrieve_data<T: PodaClientTrait>(
    request: RetrieveDataRequest,
    dispenser: Arc<Dispenser<T>>,
) -> Result<impl warp::Reply, Infallible> {
    match dispenser.retrieve_data(request.namespace, request.commitment).await {
        Ok(data) => {
            Ok(warp::reply::with_status(
                warp::reply::json(&RetrieveDataResponse {
                    success: true,
                    message: "Data retrieved successfully".to_string(),
                    data: Some(data),
                }),
                warp::http::StatusCode::OK,
            ))
        }
        Err(e) => {
            Ok(warp::reply::with_status(
                warp::reply::json(&RetrieveDataResponse {
                    success: false,
                    message: format!("Failed to retrieve data: {:?}", e),
                    data: None,
                }),
                warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            ))
        }
    }
}

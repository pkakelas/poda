use common::log::error;
use dispencer::http::{RetrieveDataRequest, RetrieveDataResponse, SubmitDataRequest, SubmitDataResponse};
use anyhow::Result;
use pod::FixedBytes;

pub async fn submit_data(dispencer_url: &str, data: &[u8]) -> Result<SubmitDataResponse> {
    let client = reqwest::Client::new();
    let url = format!("{}/submit", dispencer_url);
    let request_body = SubmitDataRequest {
        data: data.to_vec(),
    };

    let res = client.post(&url).json(&request_body).send().await?;
    if !res.status().is_success() {
        error!("Failed to submit data, status: {}", res.status());
        return Err(anyhow::anyhow!("Failed to submit data, status: {}", res.status()));
    }

    let response_body: SubmitDataResponse = res.json().await?;
    Ok(response_body)
}

pub async fn retrieve_data(dispencer_url: &str, commitment: &FixedBytes<32>) -> Result<RetrieveDataResponse> {
    let client = reqwest::Client::new();
    let url = format!("{}/retrieve", dispencer_url);
    let request_body = RetrieveDataRequest {
        commitment: *commitment,
    };

    let res = client.post(&url).json(&request_body).send().await?;
    if !res.status().is_success() {
        error!("Failed to retrieve data, status: {}", res.status());
        let response: RetrieveDataResponse = res.json().await?;
        return Err(anyhow::anyhow!(response.message));
    }

    let response_body: RetrieveDataResponse = res.json().await?;
    Ok(response_body)
}
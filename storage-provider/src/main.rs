mod storage;
mod file_storage;
mod http;
mod utils;

use std::{str::FromStr, sync::Arc};
use pod::{client::PodaClient, PrivateKeySigner, Address};
use file_storage::FileStorage;

#[tokio::main]
pub async fn main() {
    const RPC_URL: &str = "http://localhost:8545";
    const CONTRACT_ADDRESS: &str = "0xbeFb2305cF0C2726374F7dCBc3A29df59Df89fA8";
    const PRIVATE_KEY: &str = "6df79891f22b0f3c9e9fb53b966a8861fd6fef69f99772c5c4dbcf303f10d901";

    let storage = FileStorage::new("test_storage");
    let storage = Arc::new(storage);
    let signer = PrivateKeySigner::from_str(PRIVATE_KEY).unwrap();
    let contract_address = Address::from_str(CONTRACT_ADDRESS).unwrap();
    let pod = PodaClient::new(signer, RPC_URL.to_string(), contract_address).await;
    let pod = Arc::new(pod);
    let http_server = http::start_server(storage, pod, 3000);

    http_server.await;
}


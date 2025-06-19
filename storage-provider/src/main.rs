mod storage;
mod file_storage;
mod http;
mod utils;

use std::{str::FromStr, sync::Arc};
use pod::{client::PodaClient, PrivateKeySigner, Address};
use file_storage::FileStorage;
use dotenv::dotenv;

fn load_config() -> (String, Address, u16, String) {
    dotenv().ok();

    let rpc_url = std::env::var("RPC_URL").unwrap();
    let poda_address = std::env::var("PODA_ADDRESS").unwrap().parse::<Address>().unwrap();
    let port = std::env::var("STORAGE_PROVIDER_PORT").unwrap().parse::<u16>().unwrap();
    let private_key = std::env::var("STORAGE_PROVIDER_PRIVATE_KEY").unwrap();

    (rpc_url, poda_address, port, private_key)
}


#[tokio::main(flavor = "current_thread")]
pub async fn main() {
    let (rpc_url, poda_address, port, private_key) = load_config();

    let storage = FileStorage::new("test_storage");
    let storage = Arc::new(storage);
    let signer = PrivateKeySigner::from_str(&private_key).unwrap();
    let pod = PodaClient::new(signer, rpc_url.clone(), poda_address).await;
    let pod = Arc::new(pod);
    let http_server = http::start_server(storage, pod, port);

    http_server.await;
}


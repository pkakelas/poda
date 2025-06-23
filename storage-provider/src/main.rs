mod storage;
mod file_storage;
mod http;
mod utils;
mod responder;

use std::{str::FromStr, sync::Arc, time::Duration};
use pod::{client::PodaClient, PrivateKeySigner, Address};
use file_storage::FileStorage;
use dotenv::dotenv;
use common::log::{error, info, init_logging};
use crate::responder::respond_to_active_challenges;

fn load_config() -> (String, Address, u16, String, u64) {
    dotenv().ok();
    init_logging();

    let rpc_url = std::env::var("RPC_URL").unwrap();
    let poda_address = std::env::var("PODA_ADDRESS").unwrap().parse::<Address>().unwrap();
    let port = std::env::var("STORAGE_PROVIDER_PORT").unwrap().parse::<u16>().unwrap();
    let private_key = std::env::var("STORAGE_PROVIDER_PRIVATE_KEY").unwrap();
    let responder_interval = std::env::var("STORAGE_PROVIDER_RESPONDER_INTERVAL").unwrap_or("20".to_string()).parse::<u64>().unwrap();

    (rpc_url, poda_address, port, private_key, responder_interval)
}


#[tokio::main(flavor = "current_thread")]
pub async fn main() {
    let (rpc_url, poda_address, port, private_key, responder_interval) = load_config();

    let storage = FileStorage::new("test_storage");
    let storage = Arc::new(storage);

    let signer = PrivateKeySigner::from_str(&private_key).unwrap();
    let my_address = signer.address();

    let pod = PodaClient::new(signer, rpc_url.clone(), poda_address).await;
    let pod = Arc::new(pod);
    let http_server = http::start_server(storage.clone(), pod.clone(), port);

    tokio::spawn(async move {
        loop {
            match respond_to_active_challenges(&storage, &pod, my_address).await {
                Ok(()) => info!("Responding to active challenges succeeded"), 
                Err(e) => error!("Responding to active challenges failed {:?}", e)
            }

            tokio::time::sleep(Duration::from_secs(responder_interval)).await;
        }
    });

    http_server.await;
}


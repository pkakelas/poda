mod dispenser;
mod http;
use std::{str::FromStr, sync::Arc};

use http::start_server;
use dispenser::Dispenser;
use pod::{client::{PodaClient}, Address, PrivateKeySigner};
use dotenv::dotenv;
use common::log::{init_logging, info};

fn load_config() -> (String, Address, u16, String) {
    dotenv().ok();
    init_logging();

    let port = std::env::var("DISPENCER_PORT").unwrap().parse::<u16>().unwrap();
    let private_key = std::env::var("DISPENCER_PRIVATE_KEY").unwrap();
    let rpc_url = std::env::var("POD_RPC_URL").unwrap();
    let poda_address = std::env::var("PODA_ADDRESS").unwrap().parse::<Address>().unwrap();

    info!("Loading config");

    (rpc_url, poda_address, port, private_key)
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (rpc_url, poda_address, port, private_key) = load_config();

    let signer = PrivateKeySigner::from_str(&private_key).unwrap();
    let poda_client = PodaClient::new(signer, rpc_url.clone(), poda_address).await;

    let dispenser = Arc::new(Dispenser::new(poda_client));

    start_server(dispenser, port).await;
}
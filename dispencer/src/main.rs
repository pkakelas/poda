mod dispenser;
mod http;
use std::str::FromStr;

use http::start_server;
use dispenser::Dispenser;
use pod::{client::{PodaClient}, Address, PrivateKeySigner};
use dotenv::dotenv;

fn load_config() -> (String, Address, u16, String) {
    dotenv().ok();

    let port = std::env::var("PORT").unwrap().parse::<u16>().unwrap();
    let private_key = std::env::var("PRIVATE_KEY").unwrap();
    let rpc_url = std::env::var("RPC_URL").unwrap();
    let poda_address = std::env::var("PODA_ADDRESS").unwrap().parse::<Address>().unwrap();

    (rpc_url, poda_address, port, private_key)
}

#[tokio::main]
async fn main() {
    let (rpc_url, poda_address, port, private_key) = load_config();

    let signer = PrivateKeySigner::from_str(&private_key).unwrap();
    let poda_client = PodaClient::new(signer, rpc_url.clone(), poda_address).await;

    let dispenser = Dispenser::new(poda_client);

    start_server(dispenser, port).await;
}
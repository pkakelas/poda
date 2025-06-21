mod challenger;

use std::{str::FromStr, time::Duration};
use dotenv::dotenv;
use pod::{client::PodaClient, Address, PrivateKeySigner};
use common::log::init_logging;

use crate::challenger::Challenger;

fn load_config() -> (String, Address, String, usize, u64) {
    dotenv().ok();
    init_logging();

    let rpc_url = std::env::var("RPC_URL").unwrap();
    let poda_address = std::env::var("PODA_ADDRESS").unwrap().parse::<Address>().unwrap();
    let private_key = std::env::var("CHALLENGER_PRIVATE_KEY").unwrap();
    let sample_size = std::env::var("CHALLENGER_SAMPLE_SIZE").unwrap_or("10".to_string()).parse::<usize>().unwrap();
    let interval = std::env::var("CHALLENGER_INTERVAL_SECS").unwrap_or("10".to_string()).parse::<u64>().unwrap();

    (rpc_url, poda_address, private_key, sample_size, interval)
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (rpc_url, poda_address, private_key, sample_size, interval) = load_config();

    let signer = PrivateKeySigner::from_str(&private_key).unwrap();
    let pod = PodaClient::new(signer, rpc_url.clone(), poda_address).await;

    let challenger = Challenger::new(pod, sample_size, Duration::from_secs(interval));
    challenger.run().await.unwrap();
}
mod dispenser;

use std::str::FromStr;
use dispenser::Dispenser;

use pod::{client::{PodaClient, PodaClientTrait}, Address, FixedBytes, PrivateKeySigner};
use sha3::{Digest, Keccak256};

// Example usage
#[tokio::main]
async fn main() {
    let addr = "0x77E158587C3307319e69FFfA73fED83C22DdFc23".parse::<Address>().unwrap();
    let signer = PrivateKeySigner::from_str("6df79891f22b0f3c9e9fb53b966a8861fd6fef69f99772c5c4dbcf303f10d901").unwrap();
    let url = "http://localhost:8545".to_string();

    let pod = PodaClient::new(signer, url, addr).await;

    let data = b"Hello, world!";

    let commitment = Keccak256::digest(data);
    let fixed_commitment = FixedBytes::<32>::from_slice(&commitment);

    let exists = pod.get_commitment_info(fixed_commitment).await.unwrap();
    println!("Commitment exists: {:?}", exists);

    let dispenser = Dispenser::new(pod);
    dispenser.submit_data("test_namespace".to_string(), data).await.unwrap();
}
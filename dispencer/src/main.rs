mod dispenser;

use dispenser::Dispenser;
use pod::{client::{PodaClient, PodaClientTrait}, PrivateKeySigner};

// Example usage
#[tokio::main]
async fn main() {
    let PORT = 5555;
    let PRIVATE_KEY = "6df79891f22b0f3c9e9fb53b966a8861fd6fef69f99772c5c4dbcf303f10d901";
    let RPC_URL = "http://localhost:8545".to_string();
}
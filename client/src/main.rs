pub mod utils;
mod dispencer_client;

use utils::{faucet_if_needed, get_provider_for_signer, get_actors};
use clap::{Parser, Subcommand};
use common::log::{error, info, init_logging};
use common::{
    types::FixedBytes,
};
use crate::dispencer_client::{retrieve_data, submit_data};
use crate::utils::health_check;
use pod::client::PodaClientTrait;
use pod::{client::PodaClient, Address, PrivateKeySigner};
use std::{fs, str::FromStr};

#[derive(Parser)]
#[command(name = "poda-localnet")]
#[command(about = "Poda Localnet Setup Tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

static FAUCET_PRIVATE_KEY: &str = "6df79891f22b0f3c9e9fb53b966a8861fd6fef69f99772c5c4dbcf303f10d901";
static DEFAULT_RPC_URL: &str = "http://localhost:8545";
static DISPENCER_URL: &str = "http://localhost:8000";
static DEFAULT_STORAGE_PROVIDER_STAKE: u128 = 1000000000000000000;
static N_STORAGE_PROVIDERS: usize = 3; // DO NOT CHANGE THIS. IT MESSES UP EVERYTHING.

#[derive(Subcommand)]
enum Commands {
    /// Setup blockchain infrastructure (accounts, funding, contract deployment)
    Setup {
    },
    /// Get all active challenges issued from the challenger for a given address
    GetActiveChallenges {
        address: Address,
    },
    /// Get a chunk challenge for a given commitment, chunk id, and provider
    ChunkChallenge {
        commitment: String,
        chunk_id: u16,
        provider: Address,
    },
    /// Submit data to the dispenser
    SubmitData {
        data: Vec<u8>,
    },
    /// Retrieve data from the dispenser
    RetrieveData {
        commitment: String,
    },
    /// Check the health of the dispenser and storage providers
    HealthCheck {
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();
    let cli = Cli::parse();

    match &cli.command {
        Commands::Setup { } => {
            info!("🔗 Setting up Poda Blockchain Infrastructure");
            info!("==============================================");
            
            let setup_result = setup_poda_localnet(DEFAULT_RPC_URL, DEFAULT_STORAGE_PROVIDER_STAKE).await;
            
            match setup_result {
                Ok(_) => {
                    info!("✅ Setup completed successfully!");
                    info!("📁 Configuration saved to: .");
                }
                Err(e) => {
                    error!("❌ Setup failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::GetActiveChallenges { address } => {
            dotenv::dotenv().ok();

            let poda_address = std::env::var("PODA_ADDRESS").unwrap();
            let signer = PrivateKeySigner::from_str(FAUCET_PRIVATE_KEY).unwrap();
            let poda_client = PodaClient::new(signer, DEFAULT_RPC_URL.to_string(), Address::from_str(&poda_address).unwrap()).await;
            let challenges = poda_client.get_provider_active_challenges(*address).await.unwrap();
            info!("🔍 Active challenges: {:?}", challenges);
        }
        Commands::ChunkChallenge { commitment, chunk_id, provider } => {
            dotenv::dotenv().ok();

            let commitment: FixedBytes<32> = FixedBytes::from_str(commitment).unwrap();
            let poda_address = std::env::var("PODA_ADDRESS").unwrap();
            let signer = PrivateKeySigner::from_str(FAUCET_PRIVATE_KEY).unwrap();
            let poda_client = PodaClient::new(signer, DEFAULT_RPC_URL.to_string(), Address::from_str(&poda_address).unwrap()).await;
            let challenge = poda_client.get_chunk_challenge(commitment, *chunk_id, *provider).await.unwrap();
            info!("🔍 Challenge: {:?}", challenge);
        },
        Commands::SubmitData { data } => {
            let data = data.clone();
            let response = submit_data(DISPENCER_URL, &data).await;
            match response {
                Ok(response) => {
                    info!("🔍 Submitted data: [{} bytes]", data.len());
                    info!("🔍 Commitment: {}", response.commitment);
                    info!("🔍 Chunk Assignments:");
                    for (provider_id, chunk_ids) in response.assignments.iter() {
                        info!("🔍 Provider {}: {:?}", provider_id, chunk_ids);
                    }
                }
                Err(e) => {
                    error!("❌ Failed to submit data: {:?}", e);
                }
            }
        },
        Commands::RetrieveData { commitment } => {
            let commitment: FixedBytes<32> = FixedBytes::from_str(commitment).unwrap();
            let response = retrieve_data(DISPENCER_URL, &commitment).await;
            match response {
                Ok(response) => {
                    let data = response.data.unwrap();
                    info!("🔍 Retrieved data: [{} bytes]", data.len());
                    info!("🔍 Data: {:?}", data);
                }
                Err(e) => {
                    error!("❌ Failed to retrieve data: {:?}", e);
                }
            }
        },
        Commands::HealthCheck { } => {
            let response = health_check(DISPENCER_URL.to_string()).await;
            match response {
                Ok(_) => {
                    info!("🔍 Dispencer is up and running!");
                }
                Err(_) => {
                    error!("❌ Dispencer is down");
                }
            }

            for i in 0..N_STORAGE_PROVIDERS {
                let response = health_check(format!("http://localhost:{}", 8001 + i as u16)).await;
                match response {
                    Ok(_) => {
                        info!("🔍 Storage provider {} is up and running!", i + 1);
                    }
                    Err(_) => {
                        info!("❌ Storage provider {} is down", i + 1);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn setup_poda_localnet(
    rpc_url: &str, 
    storage_provider_stake: u128,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("🔍 Initializing Poda Localnet");

    let actors = get_actors();
    info!("🔍 Loaded {} actors from localnet/actors.json", actors.len());

    info!("💰 Funding service accounts so that they have more than 1.5 ETH...");
    let faucet_signer = PrivateKeySigner::from_str(FAUCET_PRIVATE_KEY).unwrap();
    info!("🔍 Faucet signer: {:?}", faucet_signer);
    let faucet_address = faucet_signer.address();
    info!("🔍 Faucet address: {:?}", faucet_address);
    let faucet = get_provider_for_signer(faucet_signer, rpc_url).await;
    faucet_if_needed(&faucet, &actors).await;
    info!("💰 Funding service accounts so that they have more than 1.5 ETH... done");

    info!("🔍 Deploying Poda contract...");
    let poda_address = PodaClient::deploy_poda(faucet, faucet_address, storage_provider_stake).await.unwrap();
    info!("🔍 Poda contract deployed at: {}", poda_address);

    info!("Registering storage providers...");
    let port_start_from = 8001; 
    for (i, actor) in actors[2..N_STORAGE_PROVIDERS + 2].iter().enumerate() {
        let signer = PrivateKeySigner::from_str(&actor.private_key).unwrap();
        let client = PodaClient::new(signer, rpc_url.to_string(), poda_address).await;
        let base_url = format!("http://host.docker.internal:{}", port_start_from + i as u16);

        let name = format!("storage-provider-{}", i);
        let res = client.register_provider(name, base_url.clone(), storage_provider_stake).await;
        if res.is_err() {
            error!("Failed to register storage provider {}: {:?}", i, res.err());
        }
        info!("Registered storage provider {} at {}", i, base_url);
    }

    info!("Network architecture:");
    info!("  - Challenger: {} with no exposed http server", actors[1].address);
    info!("  - Dispencer: {} at {}", actors[0].address, format!("http://localhost:{}", 8000));
    for (i, actor) in actors[2..N_STORAGE_PROVIDERS + 2].iter().enumerate() {
        info!("  - Storage Provider {}: {} at {}", i, actor.address, format!("http://localhost:{}", 8001 + i as u16));
    }

    info!("🔍 Generating .env file...");
    let storage_provider_private_keys = actors[2..N_STORAGE_PROVIDERS + 2].iter().map(|actor| actor.private_key.clone()).collect();
    let regenerate_env_file = generate_env_file(FAUCET_PRIVATE_KEY, FAUCET_PRIVATE_KEY, poda_address, &storage_provider_private_keys).await;
    if regenerate_env_file.is_err() {
        error!("Failed to generate .env file: {:?}", regenerate_env_file.err());
    }

    Ok(())
}

async fn generate_env_file(dispenser_private_key: &str, challenger_private_key: &str, poda_address: Address, storage_provider_private_keys: &Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let env_file = format!(
"# Blockchain Configuration
RPC_URL=http://host.docker.internal:8545
PODA_ADDRESS={}

# Service Configuration
DISPENCER_PRIVATE_KEY={}
CHALLENGER_PRIVATE_KEY={}

# Storage Provider Private Keys
STORAGE_PROVIDER_1_PRIVATE_KEY={}
STORAGE_PROVIDER_2_PRIVATE_KEY={}
STORAGE_PROVIDER_3_PRIVATE_KEY={}     ", 
        poda_address, 
        dispenser_private_key, challenger_private_key,
        storage_provider_private_keys[0],
        storage_provider_private_keys[1],
        storage_provider_private_keys[2]
    );

    fs::write(".env", env_file)?;
    Ok(())
}
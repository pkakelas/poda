use anyhow::Result;
use common::{
    constants::ONE_ETH,
};
use pod::{Address, EthereumWallet, PodProvider, PodProviderBuilder, PrivateKeySigner, Provider, U256};
use serde::Deserialize;
use serde_json;

#[derive(Deserialize)]
pub struct Actor {
    pub address: Address,
    pub private_key: String,
}

pub fn get_actors() -> Vec<Actor> {
    let actors = std::fs::read_to_string("./actors.json").unwrap();
    let actors: Vec<Actor> = serde_json::from_str(&actors).unwrap();
    actors
}

pub async fn faucet_if_needed(faucet: &PodProvider, actors: &Vec<Actor>) -> () {
    for actor in actors {
        let min_balance = U256::from(ONE_ETH) * U256::from(1.5);
        let balance = faucet.get_balance(actor.address).await.unwrap();

        if balance < min_balance {
            faucet.transfer(actor.address, U256::from(ONE_ETH)).await.unwrap();
        }
    }
}

pub async fn get_provider_for_signer(signer: PrivateKeySigner, rpc_url: &str) -> PodProvider {
    PodProviderBuilder::with_recommended_settings()
        .wallet(EthereumWallet::new(signer))
        .on_url(rpc_url.to_string())
        .await
        .expect("Failed to create provider")
}

pub async fn health_check(url: String) -> Result<()> {
    let client = reqwest::Client::new();
    let res = client.get(url + "/health").send().await?;
    if !res.status().is_success() {
        return Err(anyhow::anyhow!("Failed to check health, status: {}", res.status()));
    }

    Ok(())
}
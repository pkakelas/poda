use anyhow::Result;
use common::{
    constants::ONE_ETH,
};
use pod::{Address, EthereumWallet, PodProvider, PodProviderBuilder, PrivateKeySigner, Provider, U256};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Actor {
    pub address: Address,
    pub private_key: String,
}

pub fn get_actors() -> Vec<Actor> {
    vec![
        Actor {
            address: "0xB80dC607CaF83FDb694E4B3a233EceEA2Fe40229".parse().unwrap(),
            private_key: "0x261d08e6adc2f722855872297c4bf05839342d554a07c9c807ce6a9272fd7a65".to_string(),
        },

        Actor {
            address: "0xC44A3e14B0Ba6bA7f7C3D46D1AfDb3Ae6D4f6A87".parse().unwrap(),
            private_key: "0x20251bf7898d9ca6c5ee6aedd89b66cf1145f945981fe5aee5646a54971e8dd5".to_string(),
        },
        Actor {
            address: "0xfec3Af319104399FF144C94D3FFF0f12071eeD98".parse().unwrap(),
            private_key: "0x0591ff3da15ab810c578b52a254ea7b1a65e70b025207fd584b0408f2e56c732".to_string(),
        },
        Actor {
            address: "0x02F1dcAd9b1DD8FA85610964EEd20d1E25b255Ae".parse().unwrap(),
            private_key: "0xc514c0b0d5e63483d41edfac6f6bb1ae460041e18abf176fc49edf70e33df9b8".to_string(),
        },
        Actor {
            address: "0x98dB88e60b68fbFFAF2FE5BCbc59358Ab24c1A33".parse().unwrap(),
            private_key: "0xc43f9707845f5869bf4279295ebdd0f49c03c3acf05892fe2cfc993657565ab3".to_string(),
        },
        Actor {
            address: "0x10825CEe7C4B6D6965041152773ddCC5b0A19AD0".parse().unwrap(),
            private_key: "0x5340606464380e8dc1999ce6c8a883e4174e7c6ca2eb04090e2f05d83a56f6e4".to_string(),
        },
        Actor {
            address: "0x912c8b9Bc42E342cE54a3Ee8A8b29fF6B9D109Ad".parse().unwrap(),
            private_key: "0x64f363cf249860644b8cd7357dd434e6d8c4ec8610b08561d947c118787dc9d3".to_string(),
        },
        Actor {
            address: "0x692E9DFd53EF2667093DF0A772F223FEd4b6c47a".parse().unwrap(),
            private_key: "0x951fd7f6ff0d4dad78e0f03fef1e3189245b3ad4c3e1a9aa0bcb0b5fb64189ca".to_string(),
        }
    ]
}

pub async fn faucet_if_needed(faucet: &PodProvider, actors: &Vec<Actor>) {
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
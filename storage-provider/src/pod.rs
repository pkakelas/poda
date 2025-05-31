use pod_sdk::{
    provider::{PodProvider, PodProviderBuilder}, Address, EthereumWallet, PrivateKeySigner
};

pub struct Pod {
    pub rpc_url: String,
    pub provider: PodProvider,
    pub address: Address,
}

impl Pod {
    pub async fn new(signer: PrivateKeySigner, rpc_url: String, address: Address) -> Self {
        let provider = PodProviderBuilder::new()
            .wallet(EthereumWallet::new(signer))
            .on_url(rpc_url.clone())
            .await
            .expect("Failed to create provider");

        Self {
            rpc_url,
            provider,
            address,
        }
    }
}
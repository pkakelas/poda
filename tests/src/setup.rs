#[cfg(test)]
pub mod setup {
    #[cfg(test)]
    use {challenger::challenger::Challenger};
    use serde::Deserialize; 
    use dispencer::dispenser::Dispenser;
    use pod::{
        client::{PodaClient, PodaClientTrait},
        Address,
        EthereumWallet,
        PodProvider,
        PodProviderBuilder,
        PrivateKeySigner,
        Provider,
        U256
    };
    use common::{
        constants::ONE_ETH,
        log::{info, error, init_logging}
    };
    use std::{
        net::TcpListener,
        str::FromStr,
        sync::Arc,
        time::Duration
    };
    use storage_provider::{FileStorage};
    use tempfile::TempDir;
    use tokio::{sync::oneshot, time::sleep};

    pub struct ServerHandle {
        _temp_dir: Option<TempDir>,
        _shutdown_tx: oneshot::Sender<()>,
    }

    #[cfg(test)]
    #[derive(Deserialize, Debug)]
    pub struct Actor {
        address: Address,
        private_key: String,
    }

    #[cfg(test)]
    pub struct Setup {
        pub poda_address: Address,
        pub dispencer_handle: DispencerHandle,
        pub storage_server_handles: Vec<StorageServerHandle>,
        pub challenger: Option<Challenger>,
    }

    pub struct StorageServerHandle {
        pub storage: Arc<FileStorage>,
        pub base_url: String,
        pub name: String,
        pub owner_address: Address,
        pub pod: PodaClient,
        pub server: ServerHandle,
    }

    pub struct DispencerHandle {
        pub dispencer: Arc<Dispenser<PodaClient>>,
        pub base_url: String,
        pub owner_address: Address,
        pub server: ServerHandle
    }

    const FAUCET_PRIVATE_KEY: &str = "6df79891f22b0f3c9e9fb53b966a8861fd6fef69f99772c5c4dbcf303f10d901";
    const MIN_STAKE: u128 = ONE_ETH / 100;

    static INIT: std::sync::Once = std::sync::Once::new();

    // n_actors: Number of actors in setup. 1 will be dispencer, the rest will be storage providers
    #[cfg(test)]
    pub async fn setup_pod(n_storage_providers: usize, rpc_url: &str, with_challenger: bool) -> Setup {
        INIT.call_once(|| {
            init_logging();
        });

        let faucet = PrivateKeySigner::from_str(FAUCET_PRIVATE_KEY).expect("Invalid private key");
        let faucet_address = faucet.address();
        let faucet = get_provider_for_signer(faucet, rpc_url).await;

        info!("Deploying poda contract");
        let poda_address = PodaClient::deploy_poda(faucet.clone(), faucet_address, MIN_STAKE).await.unwrap();
        info!("Deployed poda contract at: {:?}", poda_address);

        let actors = get_actors();
        info!("Fauceting actors");
        faucet_if_needed(faucet, &actors).await;

        let mut clients: Vec<PodaClient> = Vec::new();
        for actor in actors.iter() {
            let signer = PrivateKeySigner::from_str(&actor.private_key).expect("Invalid private key");
            let client = PodaClient::new(signer, rpc_url.to_string(), poda_address).await;
            clients.push(client);
        }

        let dispencer_client = clients[0].clone();
        let dispencer_handle = start_new_dispencer_server(&dispencer_client).await;

        let challenger = if with_challenger {
            Some(Challenger::new(dispencer_client.clone(), 10, Duration::from_secs(10)))
        } else {
            None
        };

        let mut storage_server_handles: Vec<StorageServerHandle> = Vec::new();
        
        for i in 2..n_storage_providers + 2 {
            let storage_provider = clients[i].clone();
            info!("Starting storage provider server for provider: {:?}", storage_provider.signer.address());
            let name = format!("storage-provider-{}", i);
            let handle = start_new_storage_provider_server(&storage_provider, &name).await;
            let res = storage_provider.register_provider(name, handle.base_url.to_string(), ONE_ETH).await;

            if res.is_err() {
                error!("Error registering provider. Probably already registered.");
            }

            info!("Storage provider url: {:?}", handle.base_url);
            storage_server_handles.push(handle);
        }

        let providers = dispencer_client.get_providers().await.unwrap();
        info!("Providers: {:?}", providers);

        Setup {
            poda_address,
            dispencer_handle,
            storage_server_handles,
            challenger,
        }
    }

    #[cfg(test)]
    pub async fn get_provider_for_signer(signer: PrivateKeySigner, rpc_url: &str) -> PodProvider {
        PodProviderBuilder::with_recommended_settings()
            .wallet(EthereumWallet::new(signer))
            .on_url(rpc_url.to_string())
            .await
            .expect("Failed to create provider")
    }

    #[cfg(test)]
    pub fn get_actors() -> Vec<Actor> {
        let actors = std::fs::read_to_string("src/actors.json").unwrap();
        let actors: Vec<Actor> = serde_json::from_str(&actors).unwrap();
        actors
    }

    #[cfg(test)]
    async fn faucet_if_needed(faucet: PodProvider, actors: &Vec<Actor>) -> () {
        for actor in actors {
            let min_balance = U256::from(ONE_ETH) * U256::from(1.5); // 100 eth
            let balance = faucet.get_balance(actor.address).await.unwrap();

            if balance < min_balance {
                faucet.transfer(actor.address, U256::from(ONE_ETH)).await.unwrap();
            }

            let balance = faucet.get_balance(actor.address).await.unwrap();
            info!("balance of actor {:?} is {:?}", actor.address, balance);
        }
    }

    #[cfg(test)]
    async fn start_new_dispencer_server(pod: &PodaClient) -> DispencerHandle {
        // Find an available port
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener); // Close the listener so the port is free

        // Create shutdowjn channel
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let dispencer_instance = Arc::new(Dispenser::new(pod.clone()));

        // Start the server in the background
        let server = dispencer::http::start_server(dispencer_instance.clone(), port);
        let _ = tokio::spawn(async move {
            let server = server;
            tokio::select! {
                _ = server => {},
                _ = shutdown_rx => {},
            }
        });

        // Wait for server to start
        sleep(Duration::from_millis(100)).await;

        let base_url = format!("http://localhost:{}", port);

        DispencerHandle {
            base_url: base_url,
            server: ServerHandle {
                _temp_dir: None,
                _shutdown_tx: shutdown_tx,
            },
            owner_address: pod.signer.address(),
            dispencer: dispencer_instance,
        }
    }

    #[cfg(test)]
    async fn start_new_storage_provider_server(pod: &PodaClient, name: &str) -> StorageServerHandle {
        // Find an available port
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener); // Close the listener so the port is free

        // Create a temporary directory for storage
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::new(temp_dir.path());
        let storage = Arc::new(storage);

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        // Start the server in the background
        let server = storage_provider::http::start_server(storage.clone(), Arc::new(pod.clone()), port);
        let _ = tokio::spawn(async move {
            let server = server;
            tokio::select! {
                _ = server => {},
                _ = shutdown_rx => {},
            }
        });

        // Wait for server to start
        sleep(Duration::from_millis(100)).await;

        let base_url = format!("http://localhost:{}", port);

        StorageServerHandle {
            storage,
            base_url,
            owner_address: pod.signer.address(),
            server: ServerHandle {
                _temp_dir: Some(temp_dir),
                _shutdown_tx: shutdown_tx,
            },
            name: name.to_string(),
            pod: pod.clone(),
        }
    }
}

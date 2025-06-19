#[cfg(test)]
pub mod setup {
    use {
        std::fs,
        std::net::TcpListener,
        std::time::Duration,
        std::{str::FromStr, sync::Arc},
        pod::client::PodaClientTrait,
        pod::{Address, EthereumWallet, PodProvider, PodProviderBuilder, Provider, U256},
        pod::{client::PodaClient, PrivateKeySigner},
        serde::Deserialize,
        storage_provider::{FileStorage},
        dispencer::dispenser::Dispenser,
        tempfile::TempDir,
        tokio::sync::oneshot,
        tokio::time::sleep,
    };

    pub struct ServerHandle {
        _temp_dir: Option<TempDir>,
        _shutdown_tx: oneshot::Sender<()>,
        pub base_url: String,
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
        pub dispencer_handle: ServerHandle,
        pub dispencer_client: PodaClient,
        pub storage_server_handles: Vec<ServerHandle>,
    }

    const FAUCET_PRIVATE_KEY: &str = "6df79891f22b0f3c9e9fb53b966a8861fd6fef69f99772c5c4dbcf303f10d901";
    const ONE_ETH: u128 = 1000000000000000000;
    const MIN_STAKE: u128 = ONE_ETH / 100;

    // n_actors: Number of actors in setup. 1 will be dispencer, the rest will be storage providers
    #[cfg(test)]
    pub async fn setup_pod(n_actors: usize, rpc_url: &str) -> Setup {
        println!("Setting up pod");
        let faucet = PrivateKeySigner::from_str(FAUCET_PRIVATE_KEY).expect("Invalid private key");
        let faucet_address = faucet.address();
        let faucet = get_provider_for_signer(faucet, rpc_url).await;

        println!("Deploying poda contract");
        let poda_address = PodaClient::deploy_poda(faucet.clone(), faucet_address, MIN_STAKE).await.unwrap();
        println!("Deployed poda contract at: {:?}", poda_address);

        let actors = get_actors();
        println!("Fauceting actors");
        faucet_if_needed(faucet, &actors).await;

        let mut clients: Vec<PodaClient> = Vec::new();
        for actor in actors.iter() {
            let signer = PrivateKeySigner::from_str(&actor.private_key).expect("Invalid private key");
            let client = PodaClient::new(signer, rpc_url.to_string(), poda_address).await;
            clients.push(client);
        }

        let dispencer_client = clients[0].clone();
        let dispencer_handle = start_new_dispencer_server(&dispencer_client).await;

        let mut server_handles: Vec<ServerHandle> = Vec::new();
        
        for i in 1..n_actors {
            let storage_provider = clients[i].clone();
            let handle = start_new_storage_provider_server(&storage_provider).await;
            let res = storage_provider.register_provider(format!("storage-provider-{}", i), handle.base_url.to_string(), ONE_ETH / 100).await;

            if res.is_err() {
                println!("Error registering provider. Probably already registered.");
            }

            println!("Storage provider url: {:?}", handle.base_url);
            server_handles.push(handle);
        }

        let providers = dispencer_client.get_providers().await.unwrap();
        println!("Providers: {:?}", providers);

        Setup {
            poda_address,
            dispencer_handle,
            dispencer_client,
            storage_server_handles: server_handles,
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
        let actors = fs::read_to_string("src/actors.json").unwrap();
        let actors: Vec<Actor> = serde_json::from_str(&actors).unwrap();
        actors
    }

    #[cfg(test)]
    async fn faucet_if_needed(faucet: PodProvider, actors: &Vec<Actor>) -> () {
        for actor in actors {
            let min_balance = U256::from(ONE_ETH).div_ceil(U256::from(10)); // 0.1 eth
            let balance = faucet.get_balance(actor.address).await.unwrap();

            if balance < min_balance {
                faucet.transfer(actor.address, U256::from(ONE_ETH)).await.unwrap();
            }

            let balance = faucet.get_balance(actor.address).await.unwrap();
            println!("balance of actor {:?} is {:?}", actor.address, balance);
        }
    }

    #[cfg(test)]
    async fn start_new_dispencer_server(pod: &PodaClient) -> ServerHandle {
        // Find an available port
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener); // Close the listener so the port is free

        // Create shutdowjn channel
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let dispencer_instance = Dispenser::new(pod.clone());

        // Start the server in the background
        let server = dispencer::http::start_server(dispencer_instance, port);
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

        ServerHandle {
            base_url: base_url,
            _temp_dir: None,
            _shutdown_tx: shutdown_tx,
        }
    }

    #[cfg(test)]
    async fn start_new_storage_provider_server(pod: &PodaClient) -> ServerHandle {
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
        let server = storage_provider::http::start_server(storage, Arc::new(pod.clone()), port);
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

        ServerHandle {
            base_url: base_url,
            _temp_dir: Some(temp_dir),
            _shutdown_tx: shutdown_tx,
        }
    }
}

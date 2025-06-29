use async_trait::async_trait;
use mockall::automock;
use std::{time::Duration};
use alloy::{primitives::FixedBytes, sol};
use alloy::primitives::U256;
use anyhow::{Result};
use pod_sdk::{network::PodNetwork, provider::{PodProvider, PodProviderBuilder}, Address, EthereumWallet, PrivateKeySigner, Provider, Bytes};
use crate::client::Poda::PodaInstance;
pub use Poda::{ProviderInfo, Commitment, ChallengeInfo};
use common::log::info;

sol!(
    #[sol(rpc)]
    #[derive(Debug)]
    Poda,
    "../contracts/out/Poda.sol/Poda.json"
);

#[automock]
#[async_trait]
pub trait PodaClientTrait {
    async fn register_provider(&self, name: String, url: String, stake: u128) -> Result<()>;
    async fn submit_commitment(&self, commitment: FixedBytes<32>, size: u32, total_chunks: u16, required_chunks: u16, kzg_commitment: Bytes) -> Result<()>;
    async fn submit_chunk_attestations(&self, commitment: FixedBytes<32>, chunk_ids: Vec<u16>) -> Result<()>;
    async fn get_providers(&self) -> Result<Vec<ProviderInfo>>;
    async fn get_eligible_providers(&self) -> Result<Vec<ProviderInfo>>;
    async fn get_provider_info(&self, provider: Address) -> Result<ProviderInfo>;
    async fn commitment_exists(&self, commitment: FixedBytes<32>) -> Result<bool>;
    async fn is_commitment_recoverable(&self, commitment: FixedBytes<32>) -> Result<bool>;
    async fn get_commitment_info(&self, commitment: FixedBytes<32>) -> Result<(Commitment, bool)>;
    async fn get_available_chunks(&self, commitment: FixedBytes<32>) -> Result<Vec<u16>>;
    async fn get_provider_chunks(&self, commitment: FixedBytes<32>, provider: Address) -> Result<Vec<u16>>;
    async fn get_chunk_owner(&self, commitment: FixedBytes<32>, chunk_id: u16) -> Result<Address>;
    async fn is_chunk_available(&self, commitment: FixedBytes<32>, chunk_id: u16) -> Result<bool>;
    async fn get_multiple_commitment_status(&self, commitment_list: Vec<FixedBytes<32>>) -> Result<Vec<bool>>;
    async fn issue_chunk_challenge(&self, commitment: FixedBytes<32>, chunk_id: u16, provider: Address) -> Result<ChallengeInfo>;
    async fn respond_to_chunk_challenge(&self, commitment: FixedBytes<32>, chunk_id: u16, chunk_data: Bytes, proof: Vec<FixedBytes<32>>) -> Result<()>;
    async fn deploy_poda(provider: PodProvider, owner: Address, min_stake: u128) -> Result<Address>;
    async fn wait_for_availability(&self, commitment: FixedBytes<32>) -> Result<()>;
    async fn verify_chunk_proof(&self, proof: Vec<FixedBytes<32>>, root: FixedBytes<32>, chunk_index: u16, chunk_data: Bytes) -> Result<bool>;
    async fn get_provider_active_challenges(&self, provider: Address) -> Result<Vec<ChallengeInfo>>;
    async fn get_provider_expired_challenges(&self, provider: Address) -> Result<Vec<ChallengeInfo>>;
    async fn get_commitment_list(&self) -> Result<Vec<FixedBytes<32>>>;
    async fn get_chunk_challenge(&self, commitment: FixedBytes<32>, chunk_id: u16, provider: Address) -> Result<ChallengeInfo>;
    async fn is_challenge_expired(&self, commitment: FixedBytes<32>, chunk_id: u16, provider: Address) -> Result<bool>;
    async fn slash_expired_challenge(&self, commitment: FixedBytes<32>, chunk_id: u16, provider: Address) -> Result<()>;
}

#[derive(Clone)]
pub struct PodaClient {
    contract: PodaInstance<(), PodProvider, PodNetwork>,
    provider: PodProvider,
    pub signer: PrivateKeySigner,
    pub address: Address,
    #[allow(dead_code)]
    rpc_url: String,
}

impl PodaClient {
    pub async fn new(signer: PrivateKeySigner, rpc_url: String, address: Address) -> Self {
        let provider = PodProviderBuilder::with_recommended_settings()
            .wallet(EthereumWallet::new(signer.clone()))
            .on_url(rpc_url.clone())
            .await
            .expect("Failed to create provider");

        let contract: PodaInstance<(), PodProvider, PodNetwork> = Poda::new(address, provider.clone());

        Self {
            signer,
            provider,
            contract,
            rpc_url,
            address,
        }
    }
}

#[async_trait]
impl PodaClientTrait for PodaClient {
    // =============================================================================
    // PROVIDER MANAGEMENT
    // =============================================================================

    async fn register_provider(&self, name: String, url: String, stake: u128) -> Result<()> {
        let stake_wei = U256::from(stake);
        let balance = self.provider.get_balance(self.signer.address()).await?;
        if balance < stake_wei {
            return Err(anyhow::anyhow!("Insufficient balance"));
        }

        let register = self.contract.registerProvider(name, url).value(stake_wei).send().await?;

        match register.get_receipt().await {
            Ok(receipt) => {
                if receipt.status() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Submit failed: {:?}", receipt))
                }
            }
            Err(e) => Err(anyhow::anyhow!("Failed to get receipt: {}", e))
        }
    }

    // =============================================================================
    // REED-SOLOMON COMMITMENT OPERATIONS
    // =============================================================================

    async fn submit_commitment(
        &self, 
        commitment: FixedBytes<32>, 
        size: u32, 
        total_chunks: u16, 
        required_chunks: u16,
        kzg_commitment: Bytes
    ) -> Result<()> {
        let submit = self.contract.submitCommitment(commitment, size, total_chunks, required_chunks, kzg_commitment).send().await?;
        
        match submit.get_receipt().await {
            Ok(receipt) => {
                if receipt.status() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Submit failed: {:?}", receipt))
                }
            }
            Err(e) => Err(anyhow::anyhow!("Failed to get receipt: {}", e))
        }
    }

    async fn submit_chunk_attestations(&self, commitment: FixedBytes<32>, chunk_ids: Vec<u16>) -> Result<()> {
        let submit = self.contract.submitChunkAttestations(commitment, chunk_ids).send().await?;
        
        match submit.get_receipt().await {
            Ok(receipt) => {
                if receipt.status() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Submit failed: {:?}", receipt))
                }
            }
            Err(e) => Err(anyhow::anyhow!("Failed to get receipt: {}", e))
        }
    }

    // =============================================================================
    // VIEW FUNCTIONS
    // =============================================================================
    async fn get_providers(&self) -> Result<Vec<ProviderInfo>> {
        let providers = self.contract.getProviders(false).call().await?;
        let info = providers._0.to_vec();

        Ok(info)
    }

    async fn get_eligible_providers(&self) -> Result<Vec<ProviderInfo>> {
        let providers = self.contract.getProviders(true).call().await?;
        let info = providers._0.to_vec();
        Ok(info)
    }

    async fn get_provider_info(&self, provider: Address) -> Result<ProviderInfo> {
        let info = self.contract.getProviderInfo(provider).call().await?._0;
        Ok(info)
    }

    async fn commitment_exists(&self, commitment: FixedBytes<32>) -> Result<bool> {
        let exists = self.contract.commitmentExists(commitment).call().await?;
        Ok(exists._0)
    }

    async fn is_commitment_recoverable(&self, commitment: FixedBytes<32>) -> Result<bool> {
        let recoverable = self.contract.isCommitmentRecoverable(commitment).call().await?;
        Ok(recoverable._0)
    }

    async fn get_commitment_info(&self, commitment: FixedBytes<32>) -> Result<(Commitment, bool)> {
        let info = self.contract.getCommitmentInfo(commitment).call().await?;
        Ok((info._0, info.isRecoverable))
    }

    async fn get_available_chunks(&self, commitment: FixedBytes<32>) -> Result<Vec<u16>> {
        let chunks = self.contract.getAvailableChunks(commitment).call().await?;
        Ok(chunks._0)
    }

    async fn get_provider_chunks(&self, commitment: FixedBytes<32>, provider: Address) -> Result<Vec<u16>> {
        let chunks = self.contract.getProviderChunks(commitment, provider).call().await?;
        Ok(chunks._0)
    }

    async fn get_chunk_owner(&self, commitment: FixedBytes<32>, chunk_id: u16) -> Result<Address> {
        let owner = self.contract.getChunkOwner(commitment, chunk_id).call().await?;
        Ok(owner._0)
    }

    async fn is_chunk_available(&self, commitment: FixedBytes<32>, chunk_id: u16) -> Result<bool> {
        let available = self.contract.isChunkAvailable(commitment, chunk_id).call().await?;
        Ok(available._0)
    }

    async fn get_multiple_commitment_status(&self, commitment_list: Vec<FixedBytes<32>>) -> Result<Vec<bool>> {
        let statuses = self.contract.getMultipleCommitmentStatus(commitment_list).call().await?;
        Ok(statuses._0)
    }

    // =============================================================================
    // CHALLENGE SYSTEM
    // =============================================================================

    async fn is_challenge_expired(&self, commitment: FixedBytes<32>, chunk_id: u16, provider: Address) -> Result<bool> {
        let result = self.contract.isChallengeExpired(commitment, chunk_id, provider).call().await?;
        Ok(result.expired)
    }

    async fn get_provider_expired_challenges(&self, provider: Address) -> Result<Vec<ChallengeInfo>> {
        let challenges = self.contract.getProviderExpiredChallenges(provider).call().await?;
        let challenges = challenges._0.to_vec();
        Ok(challenges)
    }

    async fn slash_expired_challenge(&self, commitment: FixedBytes<32>, chunk_id: u16, provider: Address) -> Result<()> {
        let res = self.contract.slashExpiredChallenge(commitment, chunk_id, provider).send().await?;

        match res.get_receipt().await {
            Ok(receipt) => {
                if receipt.status() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Slashing failed: {:?}", receipt))
                }
            }
            Err(e) => Err(anyhow::anyhow!("Failed to get receipt: {}", e))
        }
    }

    async fn get_provider_active_challenges(&self, provider: Address) -> Result<Vec<ChallengeInfo>> {
        let challenges = self.contract.getProviderActiveChallenges(provider).call().await?;
        let challenges = challenges._0.to_vec();
        Ok(challenges)
    }

    async fn get_chunk_challenge(&self, commitment: FixedBytes<32>, chunk_id: u16, provider: Address) -> Result<ChallengeInfo> {
        let challenge = self.contract.getChunkChallenge(commitment, chunk_id, provider).call().await?;
        return Ok(challenge._0);
    }

    async fn get_commitment_list(&self) -> Result<Vec<FixedBytes<32>>> {
        let commitments = self.contract.getCommitmentList().call().await?;
        Ok(commitments._0)
    }

    async fn issue_chunk_challenge(&self, commitment: FixedBytes<32>, chunk_id: u16, provider: Address) -> Result<ChallengeInfo> {
        self.contract.issueChunkChallenge(commitment, chunk_id, provider).send().await?.watch().await?;
        return self.get_chunk_challenge(commitment, chunk_id, provider).await;
    }

    async fn verify_chunk_proof(&self, proof: Vec<FixedBytes<32>>, root: FixedBytes<32>, chunk_index: u16, chunk_data: Bytes) -> Result<bool> {
        let verify = self.contract.verifyChunkProof(proof, root, chunk_index, chunk_data).call().await?;
        Ok(verify._0)
    }

    async fn deploy_poda(provider: PodProvider, owner: Address, min_stake: u128) -> Result<Address> {
        // Use the deploy_builder to create a deployment transaction
        let deployment_tx = Poda::deploy_builder(&provider, owner, U256::from(min_stake));
        
        // Send the deployment transaction
        let pending_tx = deployment_tx.send().await?;
        
        // Get the receipt from the deployment transaction
        match pending_tx.get_receipt().await {
            Ok(receipt) => {
                if receipt.status() {
                    // Extract the deployed contract address from the receipt
                    let deployed_address = receipt.contract_address
                        .ok_or_else(|| anyhow::anyhow!("No contract address in deployment receipt"))?;
                    
                    Ok(deployed_address)
                } else {
                    Err(anyhow::anyhow!("Deployment failed: {:?}", receipt))
                }
            }
            Err(e) => Err(anyhow::anyhow!("Failed to get receipt: {}", e))
        }
    }

    async fn wait_for_availability(&self, commitment: FixedBytes<32>) -> Result<()> {
        loop {
            let (commitment_info, is_recoverable) = self.get_commitment_info(commitment).await?;
            if is_recoverable {
                info!("Commitment is recoverable with {}/{} chunks", commitment_info.availableChunks, commitment_info.totalChunks);
                return Ok(());
            }
            info!("Waiting for commitment to be recoverable... {}/{} chunks", commitment_info.availableChunks, commitment_info.totalChunks);
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn respond_to_chunk_challenge(&self, commitment: FixedBytes<32>, chunk_id: u16, chunk_data: Bytes, proof: Vec<FixedBytes<32>>) -> Result<()> {
        // Estimate gas for the transaction
        let gas_estimate = self.contract
            .respondToChunkChallenge(commitment, chunk_id, chunk_data.clone(), proof.clone())
            .estimate_gas()
            .await?; 
        
        let response = self.contract
            .respondToChunkChallenge(commitment, chunk_id, chunk_data, proof)
            .gas(gas_estimate * 2) // 2x buffer
            .send()
            .await?;
        
        match response.get_receipt().await {
            Ok(receipt) => {
                if receipt.status() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Challenge response failed: {:?}", receipt))
                }
            }
            Err(e) => Err(anyhow::anyhow!("Failed to get receipt: {}", e))
        }
    }

    // =============================================================================
    // STORAGE EFFICIENCY METRICS
    // =============================================================================

    // async fn get_storage_efficiency(&self, commitment: FixedBytes<32>) -> Result<(u128, u128, u128)> {
    //     let efficiency = self.contract.getStorageEfficiency(commitment).call().await?;
    //     Ok((efficiency.originalSize.into(), efficiency.totalStoredSize.into(), efficiency.redundancyRatio.into()))
    // }

    // async fn get_network_storage_stats(&self) -> Result<(u128, u128, u128, u128)> {
    //     let stats = self.contract.getNetworkStorageStats().call().await?;
    //     Ok((stats.totalCommitments.into(), stats.totalOriginalData.into(), stats.totalStoredData.into(), stats.averageEfficiency.into()))
    // }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const ONE_ETH: u128 = 1000000000000000000;

    const RPC_URL: &str = "http://localhost:8545";
    const CONTRACT_ADDRESS: &str = "0x0EaD13CEadcE8880F5167bFDA20C7F1A7F18217d";
    const PRIVATE_KEY: &str = "6df79891f22b0f3c9e9fb53b966a8861fd6fef69f99772c5c4dbcf303f10d901";
    use common::log::{info, error, debug};

    async fn setup_test_pod() -> PodaClient {
        let signer = PrivateKeySigner::from_str(PRIVATE_KEY).expect("Invalid private key");
        let address = pod_sdk::Address::from_str(CONTRACT_ADDRESS).expect("Invalid contract address");
        
        let pod = PodaClient::new(signer, RPC_URL.to_string(), address).await;

        info!("Pod address: {:?}", pod.address);
        
        // Register provider and wait for confirmation
        pod.register_provider("test_provider_2".to_string(), "http://localhost:8000".to_string(), ONE_ETH / 10)
            .await
            .expect("Failed to register provider");

        pod
    }

    #[tokio::test]
    async fn test_submit_and_get_commitment() {
        let pod = setup_test_pod().await;

        // Test data
        let commitment = FixedBytes::from([1u8; 32]);
        let kzg_commitment = Bytes::from([1u8; 48]);
        let size = 1024u32;
        let total_chunks = 6u16;
        let required_chunks = 4u16;

        // Submit commitment and wait for confirmation
        pod.submit_commitment(commitment, size, total_chunks, required_chunks, kzg_commitment)
            .await
            .expect("Failed to submit commitment");
            
        // Check if commitment exists
        let exists = pod.commitment_exists(commitment)
            .await
            .expect("Failed to check commitment existence");

        assert!(exists, "Commitment should exist");

        // Get commitment info
        let (commitment_info, is_recoverable) = pod.get_commitment_info(commitment)
            .await
            .expect("Failed to get commitment info");

        assert_eq!(commitment_info.size, size); // size
        assert_eq!(commitment_info.totalChunks, total_chunks); // total_chunks
        assert_eq!(commitment_info.requiredChunks, required_chunks); // required_chunks
        assert_eq!(commitment_info.availableChunks, 0); // available_chunks should be 0 initially
        assert!(!is_recoverable, "Commitment should not be recoverable initially");
    }

    #[tokio::test]
    async fn test_chunk_attestations() {
        let pod = setup_test_pod().await;

        // Test data
        let commitment = FixedBytes::from([2u8; 32]);
        let kzg_commitment = Bytes::from([2u8; 48]);
        let size = 2048u32;
        let total_chunks = 6u16;
        let required_chunks = 4u16;

        // Submit commitment
        pod.submit_commitment(commitment, size, total_chunks, required_chunks, kzg_commitment)
            .await
            .expect("Failed to submit commitment");

        // Submit chunk attestations
        let chunk_ids = vec![0u16, 1u16, 2u16, 3u16];
        pod.submit_chunk_attestations(commitment, chunk_ids.clone())
            .await
            .expect("Failed to submit chunk attestations");

        // Check available chunks
        let available_chunks = pod.get_available_chunks(commitment)
            .await
            .expect("Failed to get available chunks");

        assert_eq!(available_chunks.len(), 4);
        assert_eq!(available_chunks, chunk_ids);

        // Check if commitment is recoverable
        let recoverable = pod.is_commitment_recoverable(commitment)
            .await
            .expect("Failed to check recoverability");

        assert!(recoverable, "Commitment should be recoverable with 4 chunks");
    }

    #[tokio::test]
    async fn test_provider_info() {
        let pod = setup_test_pod().await;

        // Get provider info
        let info = pod.get_provider_info(pod.address)
            .await
            .expect("Failed to get provider info");

        assert_eq!(info.name, "test_provider_2"); // name
        assert_eq!(info.url, "http://localhost:8000"); // url
        assert!(info.active); // active should be true
    }

    #[tokio::test]
    async fn test_chunk_availability() {
        let pod = setup_test_pod().await;

        // Test data
        let commitment = FixedBytes::from([3u8; 32]);
        let kzg_commitment = Bytes::from([3u8; 48]);
        let size = 1024u32;
        let total_chunks = 6u16;
        let required_chunks = 4u16;

        // Submit commitment
        pod.submit_commitment(commitment, size, total_chunks, required_chunks, kzg_commitment)
            .await
            .expect("Failed to submit commitment");

        // Submit chunk attestations
        let chunk_ids = vec![0u16, 1u16];
        pod.submit_chunk_attestations(commitment, chunk_ids.clone())
            .await
            .expect("Failed to submit chunk attestations");

        // Check individual chunk availability
        for chunk_id in &chunk_ids {
            let available = pod.is_chunk_available(commitment, *chunk_id)
                .await
                .expect("Failed to check chunk availability");
            assert!(available, "Chunk {} should be available", chunk_id);
        }

        // Check non-existent chunk
        let available = pod.is_chunk_available(commitment, 99u16)
            .await
            .expect("Failed to check chunk availability");
        assert!(!available, "Chunk 99 should not be available");
    }

    #[tokio::test]
    async fn test_contract_connection() {
        let signer = PrivateKeySigner::from_str(PRIVATE_KEY).expect("Invalid private key");
        let address = pod_sdk::Address::from_str(CONTRACT_ADDRESS).expect("Invalid contract address");
        
        let pod = PodaClient::new(signer, RPC_URL.to_string(), address).await;
        info!("Pod address: {:?}", pod.address);
        
        // Try to check if the contract exists by calling a simple view function
        // Let's try to get the MAX_CHUNKS constant first
        match pod.contract.MAX_CHUNKS().call().await {
            Ok(max_chunks) => {
                info!("Contract is accessible. MAX_CHUNKS: {}", max_chunks._0);
            }
            Err(e) => {
                error!("Error accessing MAX_CHUNKS: {:?}", e);
                // Try a different approach - check if the contract has any functions
                error!("Contract may not have the expected interface");
            }
        }
    }

    #[tokio::test]
    async fn test_provider_status() {
        let signer = PrivateKeySigner::from_str(PRIVATE_KEY).expect("Invalid private key");
        let address = pod_sdk::Address::from_str(CONTRACT_ADDRESS).expect("Invalid contract address");
        
        let pod = PodaClient::new(signer, RPC_URL.to_string(), address).await;
        
        // Check if the provider is already registered
        let provider_address = pod.address;
        match pod.get_provider_info(provider_address).await {
            Ok(info) => {
                debug!("Provider is already registered:");
                debug!("  Name: {}", info.name);
                debug!("  URL: {}", info.url);
                debug!("  Registered at: {}", info.registeredAt);
                debug!("  Reputation: {}", info.challengeCount);
                debug!("  Active: {}", info.active);
                
                // If the provider is not active, try to register again with a different name
                if !info.active {
                    debug!("Provider is inactive, trying to register with a different name...");
                    match pod.register_provider("test_provider_active".to_string(), "http://localhost:8001".to_string(), ONE_ETH / 10).await {
                        Ok(_) => info!("Successfully registered provider with new name"),
                        Err(e) => error!("Failed to register provider: {:?}", e),
                    }
                }
            }
            Err(e) => {
                error!("Provider not registered or error: {:?}", e);
                // Try to register the provider
                info!("Attempting to register provider...");
                match pod.register_provider("test_provider_3".to_string(), "http://localhost:8000".to_string(), ONE_ETH / 10).await {
                    Ok(_) => info!("Successfully registered provider"),
                    Err(e) => error!("Failed to register provider: {:?}", e),
                }
            }
        }
    }
}
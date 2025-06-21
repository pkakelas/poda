use pod::client::{PodaClient, PodaClientTrait};
use types::{log::error, Address};
use anyhow::Result;
use crate::{storage::ChunkStorageTrait, FileStorage};
use types::log::info;

pub async fn respond_to_active_challenges(file_storage: &FileStorage, pod: &PodaClient, my_address: Address) -> Result<()> {
    info!("🫡 Responding to active challenges");

    let challenges = pod.get_provider_active_challenges(my_address).await?;
    info!("🕵️‍♂️ Found {} active challenges", challenges.len());

    for i in 0..challenges.len() {
        let challenge = &challenges[i];
        let commitment = challenge.commitment;
        let chunk_id = challenge.chunkId;

        let chunk_with_proof = file_storage.retrieve(commitment, chunk_id).await?.unwrap_or_default();

        if chunk_with_proof.0.data.is_empty() {
            error!("👺 Oooops, we lost a chunk {}, {}", commitment, chunk_id);
            error!("👺 We will not submit");
        }

        info!("🙌 Responding to challenge: {:?}, {:?}, {:?}", challenge.challenge.challengeId, commitment, chunk_id);

        let result = pod.respond_to_chunk_challenge(commitment, chunk_id, chunk_with_proof.0.data.clone().into(), chunk_with_proof.1.path.clone()).await;
        if result.is_err() {
            error!("👺 Failed to respond to challenge: {:?}, {:?}, {:?}", challenge.challenge.challengeId, commitment, chunk_id);
            continue;
        }

        info!("🍻 Respond success");
    }


    Ok(())
}
use pod::client::{PodaClient, PodaClientTrait};
use common::{log::{error, info}, types::Address};
use anyhow::Result;
use crate::{storage::ChunkStorageTrait, FileStorage};

pub async fn respond_to_active_challenges(file_storage: &FileStorage, pod: &PodaClient, my_address: Address) -> Result<()> {
    info!("🫡 Responding to active challenges");

    let challenges = pod.get_provider_active_challenges(my_address).await?;
    info!("🕵️‍♂️ Found {} active challenges", challenges.len());

    for i in 0..challenges.len() {
        let challenge = &challenges[i];
        let commitment = challenge.commitment;
        let chunk_id = challenge.chunkId;

        let chunk_with_proof = file_storage.retrieve(commitment, chunk_id).await?;
        if chunk_with_proof.is_none() {
            error!("👺 Oooops, we lost a chunk {}, {}", commitment, chunk_id);
            error!("👺 We will not submit");
        }
        let (chunk, proof) = chunk_with_proof.unwrap();

        info!("🙌 Responding to challenge: {:?}, {:?}, {:?}", challenge.challenge.challengeId, commitment, chunk_id);

        let result = pod.respond_to_chunk_challenge(commitment, chunk_id, chunk.data.clone().into(), proof.path.clone()).await;
        if result.is_err() {
            error!("👺 Failed to respond to challenge: {:?}, {:?}, {:?}", challenge.challenge.challengeId, commitment, chunk_id);
            continue;
        }

        info!("🍻 Respond success");
    }


    Ok(())
}
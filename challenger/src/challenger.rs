use std::time::Duration;
use pod::{client::{PodaClient, PodaClientTrait}, Address, FixedBytes};
use anyhow::Result;
use rand::{random_range};
use types::constants::TOTAL_SHARDS;

pub struct Challenger {
    pub pod: PodaClient,
    sample_size: usize,
    interval: Duration,
}

pub type Challenge = (FixedBytes<32>, FixedBytes<32>, u16, Address);

impl Challenger {
    pub fn new(pod: PodaClient, sample_size: usize, interval: Duration) -> Self {
        Self { pod, sample_size, interval}
    }

    pub async fn run(&self) -> Result<()> {
        loop {
            self.run_round(self.sample_size).await?;
            tokio::time::sleep(self.interval).await;
        }
    }

    pub async fn run_round(&self, sample_size: usize) -> Result<Vec<Challenge>> {
        self.slash_expired_challenges().await?;
        let challenges = self.sample_challenges(sample_size).await?;
        Ok(challenges)
    }

    pub async fn sample_challenges(&self, sample_size: usize) -> Result<Vec<Challenge>> {
        let commitment_list = self.pod.get_commitment_list().await?;

        let mut samples: Vec<(FixedBytes<32>, u16)> = vec![];

        for _ in 0..sample_size {
            let commitment = commitment_list[random_range(0..commitment_list.len())];
            let chunk_id = random_range(0..TOTAL_SHARDS as u16);

            samples.push((commitment, chunk_id));
        }

        let mut challenges: Vec<Challenge> = vec![];
        for (commitment, chunk_id) in samples {
            let provider_address = self.pod.get_chunk_owner(commitment, chunk_id).await?;
            let is_chunk_available = self.pod.is_chunk_available(commitment, chunk_id).await?;
            if !is_chunk_available {
                println!("Chunk not available: {:?}", (commitment, chunk_id));
                continue
            }

            let res =  self.pod.issue_chunk_challenge(commitment, chunk_id, provider_address).await;
            if !res.is_ok() {
                eprintln!("Issuing chunk challenge failed. It's probably already issued");
                continue
            }

            let challenge = res.unwrap();

            challenges.push((challenge.challenge.challengeId, commitment, chunk_id, provider_address));
            println!("Challenged provider {:?} with commitment {:?} and chunk {:?}", provider_address, commitment, chunk_id);
        }

        Ok(challenges)
    }

    pub async fn slash_expired_challenges(&self) -> Result<()> {
        let challenges = self.pod.get_provider_expired_challenges(self.pod.address).await?;
        
        for challenge in challenges {
            let commitment = challenge.commitment;
            let chunk_id = challenge.chunkId;
            let provider_address = challenge.challenge.challenger;

            println!("Slashing provider {:?} with expired challenge {:?}", provider_address, challenge);
            let slashed = self.pod.slash_expired_challenge(commitment, chunk_id, provider_address).await;
            if slashed.is_err() {
                eprintln!("Slashing expired challenge failed. It's probably already slashed");
                continue
            }

            println!("Slashed provider {:?} with expired challenge {:?}", provider_address, challenge);
        }

        Ok(())
    }
}
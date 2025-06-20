pub mod constants;

use alloy::primitives::Keccak256;
use serde::{Serialize, Deserialize};
pub use alloy::primitives::{FixedBytes, Bytes};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub index: u16,
    pub data: Vec<u8>,
}

impl Chunk {
    pub fn hash(&self) -> FixedBytes<32> {
        let mut hasher = Keccak256::new();
        hasher.update(&self.data);
        let hash = hasher.finalize();
        FixedBytes::from_slice(&hash[..])
    }
}

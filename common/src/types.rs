pub use alloy::primitives::{FixedBytes, Bytes, B256, Address, keccak256, U256};
pub use alloy::sol_types::SolValue;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub index: u16,
    pub data: Vec<u8>,
}

impl Chunk {
    pub fn hash(&self) -> FixedBytes<32> {
        let data = self.data.as_slice();
        keccak256((self.index, keccak256(data)).abi_encode())
    }
}
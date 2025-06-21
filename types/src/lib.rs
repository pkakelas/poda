pub mod constants;
pub mod log;

use serde::{Serialize, Deserialize};
pub use alloy::primitives::{FixedBytes, Bytes, B256, Address, keccak256, U256};
pub use alloy::sol_types::SolValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub index: u16,
    pub data: Vec<u8>,
}

impl Default for Chunk {
    fn default() -> Self {
        Self { index: 0, data: vec![] }
    }
}

impl Chunk {
    pub fn hash(&self) -> FixedBytes<32> {
        let data = self.data.as_slice();
        keccak256((self.index, keccak256(data)).abi_encode())
    }
}

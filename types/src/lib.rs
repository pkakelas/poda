pub mod constants;

use serde::{Serialize, Deserialize};
pub use alloy::primitives::{FixedBytes, Bytes};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub index: u16,
    pub data: Vec<u8>,
    pub hash: FixedBytes<32>,
}

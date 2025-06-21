pub use common::types::{Bytes, B256 as Hash, keccak256 as hash, Address};

pub trait Hashable {
    fn hash_custom(&self) -> Hash;
}

impl Hashable for Vec<u8> {
    fn hash_custom(&self) -> Hash {
        hash(self)
    }
}

impl Hashable for Bytes {
    fn hash_custom(&self) -> Hash {
        hash(self)
    }
}

impl Hashable for Address {
    fn hash_custom(&self) -> Hash {
        hash(self.0)
    }
}

impl Hashable for u128 {
    fn hash_custom(&self) -> Hash {
        hash(self.to_be_bytes())
    }
}


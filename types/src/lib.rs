pub mod constants;

use std::io::Cursor;

use alloy::primitives::{Bytes, FixedBytes};
use ark_bls12_381::G1Projective as G1;
use ark_serialize::{CanonicalSerialize, CanonicalDeserialize};
use serde::{Serialize, Deserialize};

// BLS12-381 G1 compressed point size in bytes
pub const G1_COMPRESSED_SIZE: usize = 48;

/// Example usage of KzgProof and KzgCommitment with fixed-size arrays:
/// ```
/// use types::{KzgProof, KzgCommitment, G1_COMPRESSED_SIZE};
/// use ark_bls12_381::G1Projective as G1;
/// use ark_std::UniformRand;
/// 
/// let mut rng = ark_std::test_rng();
/// let g1_point = G1::rand(&mut rng);
/// 
/// // KzgProof example
/// let proof = KzgProof::new(g1_point);
/// let proof_bytes: [u8; G1_COMPRESSED_SIZE] = proof.to_bytes().unwrap();
/// assert_eq!(proof_bytes.len(), 48);
/// let reconstructed_proof = KzgProof::from_bytes(proof_bytes).unwrap();
/// assert_eq!(proof.into_inner(), reconstructed_proof.into_inner());
/// 
/// // KzgCommitment example
/// let commitment = KzgCommitment::new(g1_point);
/// let commitment_bytes: [u8; G1_COMPRESSED_SIZE] = commitment.to_bytes().unwrap();
/// assert_eq!(commitment_bytes.len(), 48);
/// let reconstructed_commitment = KzgCommitment::from_bytes(commitment_bytes).unwrap();
/// assert_eq!(commitment.into_inner(), reconstructed_commitment.into_inner());
/// ```

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub index: u16,
    pub data: Vec<u8>,
    pub hash: FixedBytes<32>,
}

// Generic G1 serialization helper
mod g1_serde {
    use super::*;
    use serde::{Serializer, Deserializer};
    use ark_std::io::Cursor;

    pub fn serialize<S>(g1: &G1, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut bytes = Vec::new();
        g1.serialize_compressed(&mut bytes).map_err(serde::ser::Error::custom)?;
        
        // Validate that the serialized size matches the expected compressed size exactly
        if bytes.len() != G1_COMPRESSED_SIZE {
            return Err(serde::ser::Error::custom(format!(
                "G1 serialization size {} does not match expected size {}",
                bytes.len(),
                G1_COMPRESSED_SIZE
            )));
        }
        
        // Convert to fixed-size array
        let mut fixed_bytes = [0u8; G1_COMPRESSED_SIZE];
        fixed_bytes.copy_from_slice(&bytes);
        
        serializer.serialize_bytes(&fixed_bytes)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<G1, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: Vec<u8> = Vec::deserialize(deserializer)?;
        
        // Validate that the deserialized size matches the expected compressed size exactly
        if bytes.len() != G1_COMPRESSED_SIZE {
            return Err(serde::de::Error::custom(format!(
                "G1 deserialization size {} does not match expected size {}",
                bytes.len(),
                G1_COMPRESSED_SIZE
            )));
        }
        
        G1::deserialize_compressed(&mut Cursor::new(bytes)).map_err(serde::de::Error::custom)
    }
}

// Generic helper trait for G1-based types
trait G1Bytes {
    fn g1_to_bytes(g1: &G1) -> Result<[u8; G1_COMPRESSED_SIZE], String> {
        let mut bytes = Vec::new();
        g1.serialize_compressed(&mut bytes)
            .map_err(|e| format!("Failed to serialize G1: {}", e))?;
        
        if bytes.len() != G1_COMPRESSED_SIZE {
            return Err(format!(
                "G1 serialization size {} does not match expected size {}",
                bytes.len(),
                G1_COMPRESSED_SIZE
            ));
        }
        
        let mut fixed_bytes = [0u8; G1_COMPRESSED_SIZE];
        fixed_bytes.copy_from_slice(&bytes);
        Ok(fixed_bytes)
    }
    
    fn g1_from_bytes(bytes: [u8; G1_COMPRESSED_SIZE]) -> Result<G1, String> {
        G1::deserialize_compressed(&bytes[..])
            .map_err(|e| format!("Failed to deserialize G1: {}", e))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KzgProof {
    #[serde(with = "g1_serde")]
    pub proof: G1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KzgCommitment {
    #[serde(with = "g1_serde")]
    pub commitment: G1,
}

impl KzgProof {
    pub fn new(proof: G1) -> Self {
        Self { proof }
    }
    
    pub fn into_inner(self) -> G1 {
        self.proof
    }
    
    pub fn as_inner(&self) -> &G1 {
        &self.proof
    }
    
    /// Returns the proof as a fixed-size byte array
    pub fn to_bytes(&self) -> Result<[u8; G1_COMPRESSED_SIZE], String> {
        Self::g1_to_bytes(&self.proof)
    }
    
    /// Creates a KzgProof from a fixed-size byte array
    pub fn from_bytes(bytes: [u8; G1_COMPRESSED_SIZE]) -> Result<Self, String> {
        let proof = Self::g1_from_bytes(bytes)?;
        Ok(Self::new(proof))
    }
}

impl KzgCommitment {
    pub fn new(commitment: G1) -> Self {
        Self { commitment }
    }
    
    pub fn into_inner(self) -> G1 {
        self.commitment
    }
    
    pub fn as_inner(&self) -> &G1 {
        &self.commitment
    }
    
    /// Returns the commitment as a fixed-size byte array
    pub fn to_bytes(&self) -> Result<[u8; G1_COMPRESSED_SIZE], String> {
        Self::g1_to_bytes(&self.commitment)
    }
    
    /// Creates a KzgCommitment from a fixed-size byte array
    pub fn from_bytes(bytes: [u8; G1_COMPRESSED_SIZE]) -> Result<Self, String> {
        let commitment = Self::g1_from_bytes(bytes)?;
        Ok(Self::new(commitment))
    }
}

// Implement the helper trait for both types
impl G1Bytes for KzgProof {}
impl G1Bytes for KzgCommitment {}

impl From<G1> for KzgProof {
    fn from(proof: G1) -> Self {
        Self::new(proof)
    }
}

impl From<KzgProof> for G1 {
    fn from(kzg_proof: KzgProof) -> Self {
        kzg_proof.into_inner()
    }
}

impl From<G1> for KzgCommitment {
    fn from(commitment: G1) -> Self {
        Self::new(commitment)
    }
}

impl From<KzgCommitment> for G1 {
    fn from(kzg_commitment: KzgCommitment) -> Self {
        kzg_commitment.into_inner()
    }
}

impl TryFrom<Bytes> for KzgCommitment {
    type Error = String;
    fn try_from(bytes: Bytes) -> Result<Self, Self::Error> {
        let bytes = bytes.into_iter().collect::<Vec<u8>>();
        let commitment = G1::deserialize_compressed(&mut Cursor::new(bytes)).map_err(|e| format!("Failed to deserialize G1: {}", e))?;
        Ok(Self::new(commitment))
    }
}

impl TryInto<Bytes> for KzgCommitment {
    type Error = String;
    fn try_into(self) -> Result<Bytes, Self::Error> {
        let bytes = self.to_bytes().map_err(|e| format!("Failed to convert KzgCommitment to Bytes: {}", e))?;
        Ok(Bytes::from(bytes))
    }
}

/// Validates that a G1 point serializes to the expected compressed size
pub fn validate_g1_size(g1: &G1) -> Result<(), String> {
    let mut bytes = Vec::new();
    g1.serialize_compressed(&mut bytes)
        .map_err(|e| format!("Failed to serialize G1: {}", e))?;
    
    if bytes.len() != G1_COMPRESSED_SIZE {
        return Err(format!(
            "G1 serialization size {} does not match expected size {}",
            bytes.len(),
            G1_COMPRESSED_SIZE
        ));
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_std::UniformRand;

    #[test]
    fn test_g1_serialization_size_cap() {
        let mut rng = ark_std::test_rng();
        let g1_point = G1::rand(&mut rng);
        
        // Test that a valid G1 point serializes to exactly the expected size
        let kzg_proof = KzgProof::new(g1_point);
        let serialized = serde_json::to_vec(&kzg_proof).expect("Should serialize successfully");
        
        // The serialized JSON should be larger than the raw bytes due to JSON encoding
        // but the actual G1 bytes should be exactly G1_COMPRESSED_SIZE
        assert!(serialized.len() > G1_COMPRESSED_SIZE, "JSON serialization should be larger than raw bytes");
        
        // Test validation function
        assert!(validate_g1_size(&g1_point).is_ok(), "Valid G1 point should pass size validation");
    }

    #[test]
    fn test_g1_deserialization_size_cap() {
        let mut rng = ark_std::test_rng();
        let g1_point = G1::rand(&mut rng);
        let kzg_proof = KzgProof::new(g1_point);
        
        // Serialize and deserialize should work
        let serialized = serde_json::to_vec(&kzg_proof).expect("Should serialize successfully");
        let deserialized: KzgProof = serde_json::from_slice(&serialized).expect("Should deserialize successfully");
        
        // The deserialized proof should be equivalent to the original
        assert_eq!(g1_point, deserialized.into_inner());
    }

    #[test]
    fn test_g1_size_constant() {
        // Verify that our constant matches the expected BLS12-381 G1 compressed size
        assert_eq!(G1_COMPRESSED_SIZE, 48, "BLS12-381 G1 compressed size should be 48 bytes");
    }

    #[test]
    fn test_kzg_proof_to_bytes() {
        let mut rng = ark_std::test_rng();
        let g1_point = G1::rand(&mut rng);
        let kzg_proof = KzgProof::new(g1_point);
        
        // Test conversion to fixed-size array
        let bytes = kzg_proof.to_bytes().expect("Should convert to bytes successfully");
        assert_eq!(bytes.len(), G1_COMPRESSED_SIZE, "Should be exactly 48 bytes");
        
        // Test conversion back from fixed-size array
        let reconstructed = KzgProof::from_bytes(bytes).expect("Should reconstruct from bytes successfully");
        assert_eq!(g1_point, reconstructed.into_inner(), "Reconstructed proof should match original");
    }

    #[test]
    fn test_kzg_proof_roundtrip() {
        let mut rng = ark_std::test_rng();
        let g1_point = G1::rand(&mut rng);
        let original_proof = KzgProof::new(g1_point);
        
        // Round trip: proof -> bytes -> proof
        let bytes = original_proof.to_bytes().expect("Should convert to bytes");
        let reconstructed_proof = KzgProof::from_bytes(bytes).expect("Should reconstruct from bytes");
        
        assert_eq!(original_proof.into_inner(), reconstructed_proof.into_inner(), 
                   "Round trip should preserve the G1 point");
    }

    #[test]
    fn test_kzg_commitment_serialization() {
        let mut rng = ark_std::test_rng();
        let g1_point = G1::rand(&mut rng);
        let kzg_commitment = KzgCommitment::new(g1_point);
        
        // Test JSON serialization/deserialization
        let serialized = serde_json::to_vec(&kzg_commitment).expect("Should serialize successfully");
        let deserialized: KzgCommitment = serde_json::from_slice(&serialized).expect("Should deserialize successfully");
        
        assert_eq!(g1_point, deserialized.into_inner(), "Deserialized commitment should match original");
    }

    #[test]
    fn test_kzg_commitment_to_bytes() {
        let mut rng = ark_std::test_rng();
        let g1_point = G1::rand(&mut rng);
        let kzg_commitment = KzgCommitment::new(g1_point);
        
        // Test conversion to fixed-size array
        let bytes = kzg_commitment.to_bytes().expect("Should convert to bytes successfully");
        assert_eq!(bytes.len(), G1_COMPRESSED_SIZE, "Should be exactly 48 bytes");
        
        // Test conversion back from fixed-size array
        let reconstructed = KzgCommitment::from_bytes(bytes).expect("Should reconstruct from bytes successfully");
        assert_eq!(g1_point, reconstructed.into_inner(), "Reconstructed commitment should match original");
    }

    #[test]
    fn test_kzg_commitment_roundtrip() {
        let mut rng = ark_std::test_rng();
        let g1_point = G1::rand(&mut rng);
        let original_commitment = KzgCommitment::new(g1_point);
        
        // Round trip: commitment -> bytes -> commitment
        let bytes = original_commitment.to_bytes().expect("Should convert to bytes");
        let reconstructed_commitment = KzgCommitment::from_bytes(bytes).expect("Should reconstruct from bytes");
        
        assert_eq!(original_commitment.into_inner(), reconstructed_commitment.into_inner(), 
                   "Round trip should preserve the G1 point");
    }

    #[test]
    fn test_from_into_conversions() {
        let mut rng = ark_std::test_rng();
        let g1_point = G1::rand(&mut rng);
        
        // Test KzgProof conversions
        let proof = KzgProof::from(g1_point);
        let back_to_g1: G1 = proof.into();
        assert_eq!(g1_point, back_to_g1);
        
        // Test KzgCommitment conversions
        let commitment = KzgCommitment::from(g1_point);
        let back_to_g1_commitment: G1 = commitment.into();
        assert_eq!(g1_point, back_to_g1_commitment);
    }

    #[test]
    fn test_bytes_to_kzg_commitment() {
        let mut rng = ark_std::test_rng();
        let g1_point = G1::rand(&mut rng);
        let original_commitment = KzgCommitment::new(g1_point);
        
        // Convert KzgCommitment to Bytes (clone to avoid ownership issues)
        let bytes: Bytes = original_commitment.clone().try_into().expect("Should convert to bytes");
        
        // Convert Bytes back to KzgCommitment
        let reconstructed_commitment: KzgCommitment = bytes.try_into().expect("Should convert from bytes");
        
        // Verify the round-trip conversion preserves the original G1 point
        assert_eq!(original_commitment.into_inner(), reconstructed_commitment.into_inner(), 
                   "Round trip conversion should preserve the G1 point");
    }
} 
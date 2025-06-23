# Poda - Decentralized Storage Network

Poda is a decentralized storage network that provides reliable, fault-tolerant data storage using erasure coding, cryptographic proofs, and economic incentives. The system ensures data availability through a network of storage providers who stake tokens and are subject to cryptographic challenges.

## Overview

Poda implements a decentralized storage solution with the following key features:

- **Erasure Coding**: Data is split into chunks using Reed-Solomon encoding (16 required chunks, 24 total chunks)
- **Cryptographic Proofs**: KZG polynomial commitments and Merkle proofs ensure data integrity
- **Economic Incentives**: Storage providers stake tokens and face penalties for failing challenges
- **Challenge System**: Random sampling and cryptographic verification prevent data loss
- **Fault Tolerance**: Data can be recovered even if some storage providers fail

## Architecture

The Poda system consists of several interconnected components:

### Smart Contract (`contracts/`)
The core Ethereum smart contract that manages:
- Storage provider registration and staking
- Commitment submission and verification
- Chunk attestation tracking
- Challenge issuance and resolution
- Economic penalties and rewards

### Core Components

#### 1. **Dispenser** (`dispencer/`)
- Handles data submission and retrieval
- Performs erasure coding (Reed-Solomon encoding)
- Distributes chunks to storage providers
- Manages data reconstruction from available chunks

#### 2. **Storage Provider** (`storage-provider/`)
- HTTP server for chunk storage and retrieval
- Implements chunk storage interface
- Handles batch operations for efficiency
- Provides chunk data with cryptographic proofs

#### 3. **Challenger** (`challenger/`)
- Monitors storage providers for data availability
- Issues random challenges to verify chunk storage
- Handles expired challenge slashing
- Ensures economic incentives work correctly

#### 4. **KZG Module** (`kzg/`)
- Implements KZG polynomial commitment scheme
- Uses Ethereum's trusted setup ceremony
- Provides commitment, proof generation, and verification
- Ensures cryptographic integrity of stored data

#### 5. **Merkle Tree** (`merkle_tree/`)
- Generates and verifies Merkle proofs for chunks
- Provides efficient proof of inclusion
- Used for chunk verification in challenges

#### 6. **POD Client** (`pod/`)
- Ethereum client for interacting with the smart contract
- Handles all blockchain operations
- Provides typed interface to contract functions

### Supporting Infrastructure

#### **Localnet** (`localnet/`)
- Local development environment setup
- Automated contract deployment
- Account funding and provider registration
- Docker-based blockchain infrastructure

#### **Common** (`common/`)
- Shared types and constants
- Logging utilities
- Common data structures

## How It Works

### 1. Data Submission
1. User submits data to the dispenser
2. Data is erasure-coded into 24 chunks (16 required for recovery)
3. KZG commitment is generated for the entire dataset
4. Merkle tree is constructed for chunk verification
5. Commitment is submitted to the smart contract
6. Chunks are distributed to storage providers

### 2. Storage Provider Operations
1. Providers register with stake and HTTP endpoint
2. Providers receive chunks with cryptographic proofs
3. Providers attest to chunk storage on-chain
4. Providers respond to challenges with chunk data and proofs

### 3. Challenge System
1. Challenger randomly samples chunks from commitments
2. Challenges are issued to storage providers
3. Providers must respond with chunk data and Merkle proof
4. Failed responses result in stake slashing
5. Expired challenges are automatically slashed

### 4. Data Retrieval
1. Client requests data by commitment hash
2. System checks if enough chunks are available (≥16)
3. Chunks are retrieved from multiple providers
4. Data is reconstructed using Reed-Solomon decoding
5. Original data is returned to the client

## Key Parameters

- **Required Chunks**: 16 (minimum needed for recovery)
- **Total Chunks**: 24 (1.5x redundancy ratio)
- **Challenge Period**: 1 hour
- **Challenge Penalty**: 0.1 ETH
- **Minimum Stake**: Configurable (default 1 ETH)

## Technology Stack

- **Blockchain**: Ethereum (with Foundry for development)
- **Language**: Rust (backend), Solidity (smart contracts)
- **Cryptography**: KZG polynomial commitments, Reed-Solomon erasure coding
- **Networking**: HTTP APIs for provider communication
- **Development**: Docker, Foundry, Cargo workspace

## Getting Started

### Prerequisites
- Rust toolchain
- Foundry (for smart contract development)
- Docker (for local development)

### Local Development Setup

1. **Clone and build**:
```bash
git clone <repository>
cd poda
cargo build
```

2. **Setup local blockchain**:
```bash
cd localnet
cargo run -- setup
```

3. **Start services**:
```bash
# Start storage providers
cargo run -p storage-provider -- --port 8001
cargo run -p storage-provider -- --port 8002
cargo run -p storage-provider -- --port 8003

# Start dispenser
cargo run -p dispencer

# Start challenger
cargo run -p challenger
```

### Smart Contract Development

```bash
cd contracts
forge build
forge test
forge script script/Poda.s.sol --rpc-url <rpc_url> --private-key <key>
```

## Security Features

- **Cryptographic Verification**: KZG commitments and Merkle proofs ensure data integrity
- **Economic Incentives**: Stake-based penalties discourage malicious behavior
- **Random Sampling**: Unpredictable challenges prevent gaming
- **Fault Tolerance**: Erasure coding ensures data recovery despite provider failures
- **Transparent Verification**: All operations are verifiable on-chain

## Use Cases

- **Decentralized File Storage**: Reliable storage without centralized providers
- **Content Distribution**: Fault-tolerant content delivery networks
- **Data Backup**: Redundant storage with cryptographic guarantees
- **Web3 Applications**: Storage layer for decentralized applications

## Contributing

The project is organized as a Cargo workspace with the following structure:
- `contracts/` - Smart contracts and Foundry configuration
- `dispencer/` - Data distribution and retrieval service
- `storage-provider/` - Chunk storage service
- `challenger/` - Challenge monitoring service
- `kzg/` - Cryptographic commitment implementation
- `merkle_tree/` - Merkle tree utilities
- `pod/` - Ethereum client interface
- `common/` - Shared utilities and types
- `localnet/` - Local development setup
- `tests/` - End-to-end testing

## License

[License information to be added] 
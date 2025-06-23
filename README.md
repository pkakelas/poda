# Poda - Decentralized Storage Network

Poda is a POC of a decentralized storage network build on Pod. It's designed to be fault-tolerant using Reed Solomon erasure coding, have cryptographic proofs and economic incentives. The system ensures data availability through a network of storage providers who stake tokens and are subject to cryptographic challenges.

## Overview

- **Erasure Coding**: Data is split into chunks using Reed-Solomon encoding (16 required chunks, 24 total chunks)
- **Cryptographic Proofs**: KZG polynomial commitments and Merkle proofs ensure data integrity
- **Economic Incentives**: Storage providers stake tokens and face penalties for failing challenges
- **Challenge System**: Random sampling and cryptographic verification prevent data loss
- **Fault Tolerance**: Data can be recovered even if some storage providers fail

## Project Architecture & System Interactions

Poda operates as a decentralized storage network with three main components that work together to ensure reliable data storage and retrieval:

### Core components

#### **Dispenser** (`dispencer/`)
The dispenser serves as the data orchestrator that handles all client interactions and coordinates the storage process. It receives data from clients and validates input requirements before performing erasure coding to split data into 24 chunks. The dispenser generates cryptographic proofs including KZG commitments and Merkle trees, then submits commitments to the smart contract on behalf of clients. It distributes chunks to storage providers with cryptographic proofs and manages chunk assignments using stake-weighted provider selection. When clients request data, the dispenser retrieves chunks from multiple providers and reconstructs the original data.

#### **Storage Providers** (`storage-provider/`)
Storage providers act as data custodians that store chunks and respond to challenges. They register with the smart contract by staking tokens (minimum 1 ETH) and receive chunks from the dispenser with cryptographic proofs. Each provider stores chunks locally with associated Merkle proofs and verifies chunk integrity using KZG and Merkle proof validation. Providers attest to chunk storage on the blockchain and must respond to challenges by providing chunk data and proofs within the time limit. If they fail to respond correctly, they face economic penalties through stake slashing.

#### **Challenger** (`challenger/`)
The challenger functions as the data availability monitor that ensures storage providers maintain their commitments. It continuously monitors all registered commitments and randomly samples chunks from different commitments and providers. The challenger issues cryptographic challenges to storage providers and verifies challenge responses on the blockchain. When providers fail to respond or respond incorrectly, the challenger triggers automatic slashing for expired or failed challenges. It distributes slashing penalties and bounties, maintaining network integrity through economic incentives.

### Interaction flow 

#### **Data Storage Flow**
1. **Client** sends data to **Dispenser** for storage
2. **Dispenser** validates data size (minimum 16 bytes) and performs erasure coding
3. **Dispenser** generates KZG commitment and Merkle tree, then sends commitment to **Smart Contract**
4. **Dispenser** distributes chunks with cryptographic proofs to **Storage Providers**
5. **Storage Providers** verify chunk integrity using KZG and Merkle proofs, then store chunks locally
6. **Storage Providers** send chunk attestations to **Smart Contract**
7. **Smart Contract** tracks available chunks and confirms when threshold (16 chunks) is reached
8. **Smart Contract** notifies **Dispenser** that data is available for retrieval

#### **Data Retrieval Flow**
1. **Client** sends commitment hash to **Dispenser** requesting data
2. **Dispenser** queries **Smart Contract** to check if commitment is recoverable (≥16 chunks available)
3. **Smart Contract** returns recoverability status to **Dispenser**
4. **Dispenser** queries **Smart Contract** to get list of providers holding chunks for this commitment
5. **Dispenser** requests chunks from multiple **Storage Providers**
6. **Storage Providers** return chunks with Merkle proofs to **Dispenser**
7. **Dispenser** verifies chunk integrity and reconstructs original data using Reed-Solomon decoding
8. **Dispenser** sends reconstructed data to **Client**

#### **Challenge Flow**
1. **Challenger** randomly selects a commitment and chunk index, then queries **Smart Contract** for chunk owner
2. **Smart Contract** returns the provider address that owns the specified chunk
3. **Challenger** sends challenge request to **Smart Contract** for the specific chunk
4. **Smart Contract** records challenge with timestamp and sends challenge notification to **Storage Provider**
5. **Storage Provider** retrieves chunk data and Merkle proof from local storage
6. **Storage Provider** sends chunk data and proof to **Smart Contract** within time limit (1 hour)
7. **Smart Contract** verifies Merkle proof against commitment root
8. **Smart Contract** either accepts response (updates provider success count) or slashes provider stake (0.1 ETH penalty)
9. **Smart Contract** distributes slashing bounty to **Challenger** if verification fails

## Crates

Poda consists of several crates:

### Pod Smart Contract (`contracts/`)
The core Pod smart contract that manages:
- Storage provider registration and staking
- Commitment submission and verification
- Chunk attestation tracking
- Challenge issuance and resolution
- Economic penalties and rewards

### Off-chain components

#### 1. **Dispenser** (`dispencer/`)
- Handles data submission and retrieval
- Performs erasure coding (Reed-Solomon encoding)
- Distributes chunks to storage providers
- Manages data reconstruction from available chunks

#### 2. **Storage Provider** (`storage-provider/`)
- HTTP server for chunk storage and retrieval
- Implements chunk storage interface
- Handles batch operations
- Provides chunk data with cryptographic proofs

#### 3. **Challenger** (`challenger/`)
- Monitors storage providers for data availability
- Issues random challenges to verify chunk storage
- Handles expired challenge slashing
- Ensures economic incentives work correctly

### Utilities

#### 4. **KZG Module** (`kzg/`)
- Implements KZG polynomial commitment scheme
- Uses Ethereum's trusted setup ceremony
- Provides commitment, proof generation, and verification
- Ensures cryptographic integrity of stored data

#### 5. **Merkle Tree** (`merkle_tree/`)
- Based on the [Merkle Tree implementation](https://github.com/podnetwork/pod-sdk/blob/b84242de1d6c2a874d1bd01b3f8e463416ac8bdd/types/src/cryptography/merkle_tree.rs) of [pod-sdk](https://github.com/podnetwork/pod-sdk)
- Generates and verifies Merkle proofs for chunks
- Provides efficient proof of inclusion
- Used for chunk verification in challenges

#### 6. **POD Client** (`pod/`)
- Ethereum client for interacting with the smart contract
- Handles all blockchain operations
- Provides typed interface to contract functions

#### **Client** (`Client/`)
- Purely for DevEx purposes
- Set's up the configuration for a localnet
- Has a CLI tool for interacting with PODA

#### **Common** (`common/`)
- Shared types and constants
- Logging utilities
- Common data structures

## Key Parameters (Subject to change)

- **Required Chunks**: 16 (minimum needed for recovery)
- **Total Chunks**: 24 (1.5x redundancy ratio)
- **Challenge Period**: 1 hour
- **Challenge Penalty**: 0.1 ETH
- **Minimum Stake**: 1 ETH

## Getting Started

### Setup a localnet

To set up a local Poda development environment, follow these steps:

### 1.Build contracts

First you need to build the Poda smart contract. Just go on `/contracts` and `forge build`.

#### 2. Build and Setup Infrastructure

Then, build the project and run the setup command to generate configuration:

```bash
# Build the project
cargo build

# Run setup to deploy contracts and generate localnet.env file
cargo run -p client -- setup
```

The setup command will:
- Deploy the Poda smart contract to your local blockchain
- Fund service accounts (dispenser, challenger, storage providers) with ETH
- Register 3 storage providers with the smart contract
- Generate a `localnet.env` file in the root folder with all necessary configuration
- Display the to-be network architecture with addresses and endpoints

#### 3. Start Services with Docker

Start all services using docker-compose:

```bash
# Start the entire stack
docker-compose --env-file localnet.env up -d
```

**Note**: Make sure `localnet.env` exists in the root folder before running this command by running step 1.

#### 4. Verify Setup

Check that all services are running:

```bash
# Health check all services
cargo run -p client -- health-check
```

You should see confirmation that the dispenser and all storage providers are up and running.

### Interacting with Poda Localnet

#### Using the CLI Tool

The client provides a convenient CLI for interacting with the localnet:

```bash
# Submit data for storage
cargo run -p client submit-data 65 66 67 68 69 70 71 72 73 74 75 76 77 78 79 80 72 73

# Retrieve data by commitment hash
cargo run -p client -- retrieve-data <commitment_hash>

# Check active challenges for a provider
cargo run -p client -- get-active-challenges <provider_address>

# Get specific chunk challenge details
cargo run -p client -- chunk-challenge <commitment> <chunk_id> <provider_address>
```

#### Using HTTP API

Alternatively, you can interact directly with the HTTP API:

```bash
# Submit data
curl -X POST http://localhost:8000/submit \
  -H "Content-Type: application/json" \
  -d '{"data": [72, 101, 108, 108, 111, 100, 101, 102, 103, 104, 15, 23, 53, 11, 12, 13, 64]}'

# Retrieve data
curl -X POST http://localhost:8000/retrieve \
  -H "Content-Type: application/json" \
  -d '{"commitment": "<commitment_hash>"}'

# Health check
curl http://localhost:8000/health
```

## Acknowledgements

- To [@eerkaijun](https://github.com/eerkaijun/) for their [KZG implementation](https://github.com/eerkaijun).
- To [pod](http://pod.network/) for [their Merkle Tree implementation](https://github.com/podnetwork/pod-sdk/blob/b84242de1d6c2a874d1bd01b3f8e463416ac8bdd/types/src/cryptography/merkle_tree.rs#L215)
- To my mom
- To my dad

## License

This project is licensed under the GNU General Public License v3.0 - see the [LICENSE.md](LICENSE.md) file for details. 

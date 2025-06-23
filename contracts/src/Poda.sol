// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import { MerkleProof } from "./lib/MerkleProof.sol";

contract Poda {
    struct Commitment {
        uint32 size;           // Original data size in bytes
        uint32 timestamp;      // When created
        uint16 totalChunks;    // Total erasure coded chunks (n)
        uint16 requiredChunks; // Chunks needed for recovery (k)
        uint16 availableChunks; // How many unique chunks we have
        bytes kzgCommitment;   // KZG commitment
    }
    
    struct Provider {
        uint32 registeredAt;
        uint32 challengeCount;
        uint32 challengeSuccessCount;
        bool active;
        uint256 stakedAmount;
    }
    
    struct ProviderInfo {
        string name;
        address addr;
        string url;
        uint32 registeredAt;
        uint32 challengeCount;
        uint32 challengeSuccessCount;
        bool active;
        uint256 stakedAmount;
    }

    struct ChunkChallenge {
        bytes32 challengeId;
        address challenger;
        uint32 issuedAt;
    }
    
    address public owner;
    
    // Core commitment data
    bytes32[] public commitmentList;
    mapping(bytes32 => Commitment) public commitments;

    // Provider management
    address[] public providerList;
    mapping(address => Provider) public providers;
    mapping(address => string) public providerNames;
    mapping(address => string) public providerUrls;
    
    // Chunk tracking
    mapping(bytes32 => mapping(uint16 => address)) public chunkOwners; // commitment => chunkId => provider
    mapping(bytes32 => mapping(address => uint16[])) public providerChunks; // commitment => provider => chunk list
    mapping(bytes32 => uint16[]) public availableChunkList; // commitment => list of available chunks
    
    // Bit-packed chunk availability for gas efficiency
    mapping(bytes32 => mapping(uint256 => uint256)) public chunkAvailability; // commitment => word => bitfield
    
    // Challenge system (per chunk)
    mapping(bytes32 => mapping(uint16 => mapping(address => ChunkChallenge))) public activeChunkChallenges;

    // Constants
    uint16 public constant MAX_CHUNKS = 1024; // Support up to 1024 chunks
    uint16 public constant MIN_REDUNDANCY_RATIO = 150; // 1.5x (k=4, n=6)
    uint256 public constant CHALLENGE_PENALTY = 0.1 ether;
    uint32 public constant CHALLENGE_PERIOD = 1 hours;
    uint32 public constant SLASH_PENALTY_PERCENTAGE = 10; // 10% of the challenge penalty distributed to the slasher
    uint256 public minStake;

    // =============================================================================
    // EVENTS
    // =============================================================================
    
    event CommitmentCreated(bytes32 indexed commitment, uint32 size, uint16 totalChunks, uint16 requiredChunks);
    event ChunkAttestation(bytes32 indexed commitment, address indexed provider, uint16 chunkId);
    event CommitmentReady(bytes32 indexed commitment, uint16 availableChunks);
    event ChunkChallengeIssued(bytes32 indexed challengeId, bytes32 indexed commitment, uint16 chunkId, address indexed provider);

    // =============================================================================
    // MODIFIERS (unchanged)
    // =============================================================================
    
    modifier onlyRegisteredProvider() {
        require(providers[msg.sender].active, "Provider not registered or inactive");
        _;
    }
    
    modifier validCommitment(bytes32 commitment) {
        require(commitments[commitment].timestamp > 0, "Commitment does not exist");
        _;
    }

    constructor(address _owner, uint256 _minStake) {
        owner = _owner;
        minStake = _minStake;
    }

    // =============================================================================
    // PROVIDER MANAGEMENT (unchanged from original)
    // =============================================================================
    
    function registerProvider(string calldata name, string calldata url) external payable {
        require(providers[msg.sender].registeredAt == 0, "Provider already registered");
        require(bytes(name).length > 0 && bytes(name).length <= 32, "Invalid name length");
        require(bytes(url).length > 0 && bytes(url).length <= 128, "Invalid URL length");
        require(msg.value >= minStake, "Insufficient stake");

        providerList.push(msg.sender);
        providers[msg.sender] = Provider({
            registeredAt: uint32(block.timestamp),
            challengeCount: 0,
            challengeSuccessCount: 0,
            active: true,
            stakedAmount: msg.value
        });
        
        providerNames[msg.sender] = name;
        providerUrls[msg.sender] = url;
    }

    // =============================================================================
    // REED-SOLOMON COMMITMENT OPERATIONS
    // =============================================================================
    
    function submitCommitment(
        bytes32 commitment,
        uint32 size,
        uint16 totalChunks,    // n (encoded chunks)
        uint16 requiredChunks, // k (original chunks)
        bytes calldata kzgCommitment
    ) external {
        require(commitment != bytes32(0), "Invalid commitment");
        require(commitments[commitment].timestamp == 0, "Commitment already exists");
        require(size > 0, "Size must be greater than 0");
        require(totalChunks > requiredChunks, "Invalid Reed-Solomon parameters");
        require(totalChunks <= MAX_CHUNKS, "Too many chunks");
        require(kzgCommitment.length == 48, "Invalid KZG commitment length");

        // Validate redundancy ratio (prevent wasteful encoding)
        require(
            (totalChunks * 100) / requiredChunks >= MIN_REDUNDANCY_RATIO,
            "Insufficient redundancy"
        );

        commitmentList.push(commitment);
        commitments[commitment] = Commitment({
            size: size,
            timestamp: uint32(block.timestamp),
            totalChunks: totalChunks,
            requiredChunks: requiredChunks,
            availableChunks: 0,
            kzgCommitment: kzgCommitment
        });
        
        emit CommitmentCreated(commitment, size, totalChunks, requiredChunks);
    }
    
    function submitChunkAttestations(
        bytes32 commitment, 
        uint16[] calldata chunkIds
    ) external onlyRegisteredProvider validCommitment(commitment) {
        require(chunkIds.length > 0 && chunkIds.length <= 50, "Invalid chunk count"); // Limit batch size
        
        Commitment storage comm = commitments[commitment];
        uint16 newChunks = 0;
        
        for (uint256 i = 0; i < chunkIds.length;) {
            uint16 chunkId = chunkIds[i];
            require(chunkId < comm.totalChunks, "Invalid chunk ID");
            require(chunkOwners[commitment][chunkId] == address(0), "Chunk already attested");
            
            // Record chunk ownership
            chunkOwners[commitment][chunkId] = msg.sender;
            providerChunks[commitment][msg.sender].push(chunkId);
            
            // Update bit-packed availability
            uint256 wordIndex = chunkId / 256;
            uint256 bitIndex = chunkId % 256;
            chunkAvailability[commitment][wordIndex] |= (1 << bitIndex);
            
            // Add to available chunk list if it's a new chunk
            availableChunkList[commitment].push(chunkId);
            newChunks++;
            
            emit ChunkAttestation(commitment, msg.sender, chunkId);
            
            unchecked { ++i; }
        }
        
        // Update available chunk count
        comm.availableChunks += newChunks;
        
        // Check if commitment is now recoverable
        if (comm.availableChunks >= comm.requiredChunks) {
            emit CommitmentReady(commitment, comm.availableChunks);
        }
    }

    function getProviderInfo(address provider) public view returns (
        ProviderInfo memory
    ) {
        Provider memory prov = providers[provider];

        return ProviderInfo({
            name: providerNames[provider],
            addr: provider,
            url: providerUrls[provider],
            registeredAt: prov.registeredAt,
            stakedAmount: prov.stakedAmount,
            challengeCount: prov.challengeCount,
            challengeSuccessCount: prov.challengeSuccessCount,
            active: prov.active
        });
    }

    function getProviders(bool eligible) external view returns (
        ProviderInfo[] memory
    ) {
        ProviderInfo[] memory providerArray = new ProviderInfo[](providerList.length);
        for (uint256 i = 0; i < providerList.length; i++) {
            if (eligible && (!providers[providerList[i]].active || providers[providerList[i]].stakedAmount < minStake)) {
                continue;
            }

            providerArray[i] = getProviderInfo(providerList[i]);
        }
        return providerArray;
    }

    function commitmentExists(bytes32 commitment) external view returns (bool) {
        return commitments[commitment].timestamp > 0;
    }

    function isCommitmentRecoverable(bytes32 commitment) external view returns (bool) {
        Commitment memory comm = commitments[commitment];
        return comm.timestamp > 0 && comm.availableChunks >= comm.requiredChunks;
    }

    function addStake(uint256 amount) external {
        require(providers[msg.sender].active, "Provider not registered or inactive");
        require(amount > 0, "Amount must be greater than 0");
        providers[msg.sender].stakedAmount += amount;
    }

    function withdrawStake(uint256 amount) external {
        require(providers[msg.sender].active, "Provider not registered or inactive");
        require(amount > 0, "Amount must be greater than 0");
        require(providers[msg.sender].stakedAmount >= amount, "Insufficient stake");
        require(providers[msg.sender].challengeCount > 0, "Provider has no active challenges");
        providers[msg.sender].stakedAmount -= amount;
        payable(msg.sender).transfer(amount);
    }

    function getCommitmentInfo(bytes32 commitment) external view returns (
        Commitment memory,
        bool isRecoverable
    ) {
        Commitment memory comm = commitments[commitment];
        return (comm, comm.availableChunks >= comm.requiredChunks);
    }

    function getAvailableChunks(bytes32 commitment) external view returns (uint16[] memory) {
        return availableChunkList[commitment];
    }

    function getCommitmentList() external view returns (bytes32[] memory) {
        return commitmentList;
    }
    
    function getProviderChunks(bytes32 commitment, address provider) external view returns (uint16[] memory) {
        return providerChunks[commitment][provider];
    }
    
    function getChunkOwner(bytes32 commitment, uint16 chunkId) external view returns (address) {
        return chunkOwners[commitment][chunkId];
    }
    
    function isChunkAvailable(bytes32 commitment, uint16 chunkId) external view returns (bool) {
        uint256 wordIndex = chunkId / 256;
        uint256 bitIndex = chunkId % 256;
        return (chunkAvailability[commitment][wordIndex] & (1 << bitIndex)) != 0;
    }
    
    function getMultipleCommitmentStatus(bytes32[] calldata cmList) external view returns (bool[] memory) {
        uint256 length = cmList.length;
        bool[] memory statuses = new bool[](length);
        
        for (uint256 i = 0; i < length;) {
            Commitment memory comm = commitments[cmList[i]];
            statuses[i] = comm.timestamp > 0 && comm.availableChunks >= comm.requiredChunks;
            unchecked { ++i; }
        }
        
        return statuses;
    }

    // =============================================================================
    // CHALLENGE SYSTEM
    // =============================================================================
    
    // Helper struct for challenge queries
    struct ChallengeInfo {
        ChunkChallenge challenge;
        bytes32 commitment;
        uint16 chunkId;
    }
    
    function issueChunkChallenge(
        bytes32 commitment, 
        uint16 chunkId, 
        address provider
    ) external validCommitment(commitment) returns (bytes32 challengeId) {
        require(chunkOwners[commitment][chunkId] == provider, "Provider doesn't own this chunk");
        require(activeChunkChallenges[commitment][chunkId][provider].challengeId == bytes32(0), "Challenge already active");
        
        challengeId = keccak256(abi.encodePacked(commitment, chunkId, provider, block.timestamp, msg.sender));


        activeChunkChallenges[commitment][chunkId][provider] = ChunkChallenge({
            challengeId: challengeId,
            challenger: msg.sender,
            // TODO: Check with pod team if this is feasible
            issuedAt: uint32(block.timestamp)
        });

        providers[provider].challengeCount++;

        emit ChunkChallengeIssued(challengeId, commitment, chunkId, provider);
    }

    function getCommitmentChunkMap(bytes32 commitment) external view returns (
        address[] memory _providers,
        uint16[][] memory _chunks
    ) {
        // Count active providers for this commitment
        uint256 activeProviders = 0;
        for (uint256 i = 0; i < providerList.length; i++) {
            if (providerChunks[commitment][providerList[i]].length > 0) {
                activeProviders++;
            }
        }
        
        _providers = new address[](activeProviders);
        _chunks = new uint16[][](activeProviders);
        
        uint256 index = 0;
        for (uint256 i = 0; i < providerList.length; i++) {
            address provider = providerList[i];
            uint16[] memory providerChunkList = providerChunks[commitment][provider];
            
            if (providerChunkList.length > 0) {
                _providers[index] = provider;
                _chunks[index] = providerChunkList;
                index++;
            }
        }
    }

    /**
     * @dev Internal helper to get challenges for a provider based on a filter function
     * @param provider The provider address
     * @param filterFunction Function to determine if a challenge should be included
     * @return Array of ChallengeInfo structs containing challenge data
     */
    function _getProviderChallenges(
        address provider,
        function(bytes32, uint16, address) view returns (bool) filterFunction
    ) internal view returns (ChallengeInfo[] memory) {
        uint256 count = 0;
        for (uint256 i = 0; i < commitmentList.length; i++) {
            bytes32 commitment = commitmentList[i];
            uint16[] memory chunks = providerChunks[commitment][provider];
            
            for (uint256 j = 0; j < chunks.length; j++) {
                if (filterFunction(commitment, chunks[j], provider)) {
                    count++;
                }
            }
        }
        
        ChallengeInfo[] memory challenges = new ChallengeInfo[](count);
        uint256 index = 0;
        
        for (uint256 i = 0; i < commitmentList.length; i++) {
            bytes32 commitment = commitmentList[i];
            uint16[] memory chunks = providerChunks[commitment][provider];
            
            for (uint256 j = 0; j < chunks.length; j++) {
                if (filterFunction(commitment, chunks[j], provider)) {
                    challenges[index] = ChallengeInfo({
                        challenge: activeChunkChallenges[commitment][chunks[j]][provider],
                        commitment: commitment,
                        chunkId: chunks[j]
                    });
                    index++;
                }
            }
        }
        
        return challenges;
    }

    function getChunkChallenge(bytes32 commitment, uint16 chunkId, address provider) external view returns (ChallengeInfo memory) {
        require(activeChunkChallenges[commitment][chunkId][provider].challengeId != bytes32(0), "No active challenge");

        return ChallengeInfo({
            challenge: activeChunkChallenges[commitment][chunkId][provider],
            commitment: commitment,
            chunkId: chunkId
        });
    }

    // TODO: Remove this function once we have a better way to get expired challenges
    function getProviderExpiredChallenges(address provider) external view returns (ChallengeInfo[] memory) {
        return _getProviderChallenges(provider, isChallengeExpired);
    }

    // TODO: Remove this function once we have a better way to get active challenges
    function getProviderActiveChallenges(address provider) external view returns (ChallengeInfo[] memory) {
        return _getProviderChallenges(provider, isChallengeActive);
    }

    function isChallengeActive(
        bytes32 commitment,
        uint16 chunkId,
        address provider
    ) internal view returns (bool) {
        ChunkChallenge memory challenge = activeChunkChallenges[commitment][chunkId][provider];
        return challenge.challengeId != bytes32(0) && challenge.issuedAt + CHALLENGE_PERIOD > block.timestamp;
    }

    function respondToChunkChallenge(
        bytes32 commitment,
        uint16 chunkId,
        bytes calldata chunkData,
        bytes32[] calldata proof
    ) external onlyRegisteredProvider {
        ChunkChallenge storage challenge = activeChunkChallenges[commitment][chunkId][msg.sender];
        require(challenge.challengeId != bytes32(0), "No active challenge");
        require(proof.length > 0, "Invalid proof");
        require(activeChunkChallenges[commitment][chunkId][msg.sender].issuedAt + CHALLENGE_PERIOD > block.timestamp, "Challenge expired");

        if (verifyChunkProof(proof, commitment, chunkId, chunkData)) {
            providers[msg.sender].challengeSuccessCount++;
        }
        else {
            slashProviderChunk(challenge, commitment, chunkId, msg.sender);
        }

        delete activeChunkChallenges[commitment][chunkId][msg.sender];
    }
    
    function verifyChunkProof(
        bytes32[] calldata proof,
        bytes32 root,
        uint16 chunkIndex,
        bytes calldata chunkData
    ) public pure returns (bool) {
        bytes32 chunkHash = keccak256(chunkData);
        bytes32 leaf = keccak256(abi.encode(chunkIndex, chunkHash));
        return MerkleProof.verify(proof, root, leaf);
    }
    
    function slashProviderChunk(
        ChunkChallenge memory challenge,
        bytes32 commitment,
        uint16 chunkId,
        address provider
    ) internal {
        require(challenge.challengeId != bytes32(0), "No active challenge");
        
        // Remove chunk from provider
        chunkOwners[commitment][chunkId] = address(0);
        
        // Update availability count
        Commitment storage comm = commitments[commitment];
        comm.availableChunks--;
        
        // Update bit-packed availability
        uint256 wordIndex = chunkId / 256;
        uint256 bitIndex = chunkId % 256;
        chunkAvailability[commitment][wordIndex] &= ~(1 << bitIndex);
        
        if (providers[provider].stakedAmount >= CHALLENGE_PENALTY) {
            providers[provider].stakedAmount -= CHALLENGE_PENALTY;
        } else {
            // If insufficient stake, mark provider as inactive
            providers[provider].active = false;
            providers[provider].stakedAmount = 0;
        }
    }

    function isChallengeExpired(
        bytes32 commitment,
        uint16 chunkId,
        address provider
    ) public view returns (bool expired) {
        ChunkChallenge memory challenge = activeChunkChallenges[commitment][chunkId][provider];
        
        if (challenge.challengeId == bytes32(0)) {
            return false;
        }
        
        return block.timestamp > challenge.issuedAt + CHALLENGE_PERIOD;
    }

    function slashExpiredChallenge(
        bytes32 commitment,
        uint16 chunkId,
        address provider
    ) external {
        ChunkChallenge storage challenge = activeChunkChallenges[commitment][chunkId][provider];
        require(challenge.challengeId != bytes32(0), "No active challenge");
        require(block.timestamp > challenge.issuedAt + CHALLENGE_PERIOD, "Challenge not expired yet");
        
        slashProviderChunk(challenge, commitment, chunkId, provider);
        
        delete activeChunkChallenges[commitment][chunkId][provider];

        payable(msg.sender).transfer(CHALLENGE_PENALTY / 10); // 10% bounty
    }

    // =============================================================================
    // STORAGE EFFICIENCY METRICS
    // =============================================================================
   
    function getStorageEfficiency(bytes32 commitment) external view returns (
        uint256 originalSize,
        uint256 totalStoredSize,
        uint256 redundancyRatio // in basis points (10000 = 100%)
    ) {
        Commitment memory comm = commitments[commitment];
        require(comm.timestamp > 0, "Commitment does not exist");
        
        originalSize = comm.size;
        totalStoredSize = (comm.size * comm.totalChunks) / comm.requiredChunks;
        redundancyRatio = (comm.totalChunks * 10000) / comm.requiredChunks;
        
        return (originalSize, totalStoredSize, redundancyRatio);
    }
}
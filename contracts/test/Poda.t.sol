// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "forge-std/Test.sol";
import "../src/Poda.sol";

contract PodaTest is Test {
    Poda public poda;

    // Test accounts
    address owner = makeAddr("owner");
    address alice = makeAddr("alice");
    address bob = makeAddr("bob");
    address charlie = makeAddr("charlie");
    address dave = makeAddr("dave");
    address eve = makeAddr("eve");
    
    // Test data
    bytes32 constant COMMITMENT_1 = keccak256("test_data_1");
    bytes32 constant COMMITMENT_2 = keccak256("test_data_2");
    bytes32 constant COMMITMENT_3 = keccak256("test_data_3");
    bytes constant KZG_COMMITMENT_1 = hex"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f";
    
    string constant NAMESPACE_1 = "rollup-optimism";
    string constant NAMESPACE_2 = "rollup-arbitrum";
    
    // Reed-Solomon test parameters
    uint16 constant TOTAL_CHUNKS = 6;    // n
    uint16 constant REQUIRED_CHUNKS = 4; // k
    uint32 constant DATA_SIZE = 1024;

    function setUp() public {
        // Fund accounts
        vm.deal(owner, 100 ether);
        vm.deal(alice, 100 ether);
        vm.deal(bob, 100 ether);
        vm.deal(charlie, 100 ether);
        vm.deal(dave, 100 ether);
        vm.deal(eve, 100 ether);

        poda = new Poda(alice, 1000);
        
        // Register providers
        vm.prank(alice);
        // payable with 1 eth
        poda.registerProvider{value: 1 ether}("Provider Alice", "https://alice.com");
        
        vm.prank(bob);
        poda.registerProvider{value: 1 ether}("Provider Bob", "https://bob.com");
        
        vm.prank(charlie);
        poda.registerProvider{value: 1 ether}("Provider Charlie", "https://charlie.com");
    }

    // =============================================================================
    // PROVIDER REGISTRATION TESTS (Same as before)
    // =============================================================================
    
    function test_RegisterProvider() public view {
        // Check Alice's registration
        Poda.ProviderInfo memory info = poda.getProviderInfo(alice);
        
        assertEq(info.name, "Provider Alice");
        assertEq(info.url, "https://alice.com");
        assertEq(info.stakedAmount, 1 ether);
        assertEq(info.challengeCount, 0);
        assertEq(info.challengeSuccessCount, 0);
        assertTrue(info.active);
        assertGt(info.registeredAt, 0);
        
        // Check provider struct
        (uint32 regAt, uint256 challengeCount, uint32 challengeSuccessCount, bool active, uint256 stakedAmount) = poda.providers(alice);
        assertEq(regAt, info.registeredAt);
        assertEq(challengeCount, info.challengeCount);
        assertEq(challengeSuccessCount, info.challengeSuccessCount);
        assertEq(active, info.active);
        assertEq(stakedAmount, info.stakedAmount);

    }
    
    function test_RegisterProvider_Duplicate() public {
        vm.prank(alice);
        vm.expectRevert("Provider already registered");
        poda.registerProvider("Duplicate Alice", "https://duplicate.com");
    }
    
    function test_RegisterProvider_InvalidName() public {
        vm.prank(dave);
        vm.expectRevert("Invalid name length");
        poda.registerProvider("", "https://dave.com");
        
        vm.prank(dave);
        vm.expectRevert("Invalid name length");
        poda.registerProvider("ThisNameIsTooLongAndExceedsThirtyTwoCharacterLimit", "https://dave.com");
    }
    
    function test_RegisterProvider_InvalidURL() public {
        vm.prank(dave);
        vm.expectRevert("Invalid URL length");
        poda.registerProvider("Dave", "");
        
        // Create URL that's definitely longer than 128 characters
        string memory longUrl = "https://this-is-a-very-very-very-very-very-very-very-very-very-very-very-very-very-very-very-very-very-very-very-very-very-long-url.com";
        require(bytes(longUrl).length > 128, "Test URL not long enough");
        
        vm.prank(dave);
        vm.expectRevert("Invalid URL length");
        poda.registerProvider("Dave", longUrl);
    }

    // =============================================================================
    // REED-SOLOMON COMMITMENT TESTS
    // =============================================================================
    
    function test_SubmitCommitment() public {
        vm.prank(alice);
        vm.expectEmit(true, false, false, true);
        emit Poda.CommitmentCreated(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS);
        poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS, KZG_COMMITMENT_1);
        
        // Check commitment was stored
        (Poda.Commitment memory commitment, bool isRecoverable) = poda.getCommitmentInfo(COMMITMENT_1);
        
        assertEq(commitment.size, DATA_SIZE);
        assertGt(commitment.timestamp, 0);
        assertEq(commitment.totalChunks, TOTAL_CHUNKS);
        assertEq(commitment.requiredChunks, REQUIRED_CHUNKS);
        assertEq(commitment.availableChunks, 0);
        assertFalse(isRecoverable);
        
        assertTrue(poda.commitmentExists(COMMITMENT_1));
        assertFalse(poda.isCommitmentRecoverable(COMMITMENT_1));
    }
    
    function test_SubmitCommitment_InvalidReedSolomonParams() public {
        vm.startPrank(alice);
        
        // totalChunks <= requiredChunks
        vm.expectRevert("Invalid Reed-Solomon parameters");
        poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, 4, 4, KZG_COMMITMENT_1);
        
        // Insufficient redundancy (less than 1.5x)
        vm.expectRevert("Insufficient redundancy");
        poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, 5, 4, KZG_COMMITMENT_1); // 1.25x ratio
        
        // Too many chunks
        vm.expectRevert("Too many chunks");
        poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, 2000, 1000, KZG_COMMITMENT_1);

        vm.stopPrank();
    }
    
    // function test_SubmitCommitment_UnregisteredProvider() public {
    //     vm.prank(dave); // Unregistered
    //     vm.expectRevert("Provider not registered or inactive");
    //     poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS);
    // }

    // =============================================================================
    // CHUNK ATTESTATION TESTS
    // =============================================================================
    
    function test_SubmitChunkAttestations() public {
        // Create commitment first
        vm.prank(alice);
        poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS, KZG_COMMITMENT_1);
        
        // Submit chunk attestations
        uint16[] memory chunks = new uint16[](2);
        chunks[0] = 0;
        chunks[1] = 1;
        
        vm.prank(bob);
        vm.expectEmit(true, true, false, false);
        emit Poda.ChunkAttestation(COMMITMENT_1, bob, 0);
        poda.submitChunkAttestations(COMMITMENT_1, chunks);
        
        // Check chunk ownership
        assertEq(poda.getChunkOwner(COMMITMENT_1, 0), bob);
        assertEq(poda.getChunkOwner(COMMITMENT_1, 1), bob);
        assertTrue(poda.isChunkAvailable(COMMITMENT_1, 0));
        assertTrue(poda.isChunkAvailable(COMMITMENT_1, 1));
        
        // Check provider chunks
        uint16[] memory bobChunks = poda.getProviderChunks(COMMITMENT_1, bob);
        assertEq(bobChunks.length, 2);
        assertEq(bobChunks[0], 0);
        assertEq(bobChunks[1], 1);
        
        // Check commitment state
        (Poda.Commitment memory commitment,) = poda.getCommitmentInfo(COMMITMENT_1);
        assertEq(commitment.availableChunks, 2);
        assertFalse(poda.isCommitmentRecoverable(COMMITMENT_1)); // Need 4 chunks
    }
    
    function test_SubmitChunkAttestations_InvalidChunkId() public {
        vm.prank(alice);
        poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS, KZG_COMMITMENT_1);
        
        uint16[] memory invalidChunks = new uint16[](1);
        invalidChunks[0] = TOTAL_CHUNKS; // Beyond range
        
        vm.prank(bob);
        vm.expectRevert("Invalid chunk ID");
        poda.submitChunkAttestations(COMMITMENT_1, invalidChunks);
    }
    
    function test_SubmitChunkAttestations_DuplicateChunk() public {
        vm.prank(alice);
        poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS, KZG_COMMITMENT_1);
        
        uint16[] memory chunks = new uint16[](1);
        chunks[0] = 0;
        
        // First attestation
        vm.prank(bob);
        poda.submitChunkAttestations(COMMITMENT_1, chunks);
        
        // Duplicate chunk
        vm.prank(charlie);
        vm.expectRevert("Chunk already attested");
        poda.submitChunkAttestations(COMMITMENT_1, chunks);
    }
    
    function test_CommitmentRecoverable() public {
        // Create commitment requiring 4 chunks
        vm.prank(alice);
        poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS, KZG_COMMITMENT_1);
        
        // Submit chunks from different providers
        uint16[] memory aliceChunks = new uint16[](2);
        aliceChunks[0] = 0;
        aliceChunks[1] = 1;
        vm.prank(alice);
        poda.submitChunkAttestations(COMMITMENT_1, aliceChunks);
        assertFalse(poda.isCommitmentRecoverable(COMMITMENT_1));
        
        uint16[] memory bobChunks = new uint16[](1);
        bobChunks[0] = 2;
        vm.prank(bob);
        poda.submitChunkAttestations(COMMITMENT_1, bobChunks);
        assertFalse(poda.isCommitmentRecoverable(COMMITMENT_1));
        
        // Fourth chunk makes it recoverable
        uint16[] memory charlieChunks = new uint16[](1);
        charlieChunks[0] = 3;
        vm.prank(charlie);
        vm.expectEmit(true, false, false, true);
        emit Poda.CommitmentReady(COMMITMENT_1, 4);
        poda.submitChunkAttestations(COMMITMENT_1, charlieChunks);
        
        assertTrue(poda.isCommitmentRecoverable(COMMITMENT_1));
        
        // Check final state
        (Poda.Commitment memory commitment, bool isRecoverable) = poda.getCommitmentInfo(COMMITMENT_1);
        assertEq(commitment.availableChunks, 4);
        assertTrue(isRecoverable);
    }

    // =============================================================================
    // CHUNK CHALLENGE SYSTEM TESTS
    // =============================================================================
    
    function test_IssueChunkChallenge() public {
        // Setup: commitment with chunk attestation
        vm.prank(alice);
        poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS, KZG_COMMITMENT_1);
        
        uint16[] memory chunks = new uint16[](1);
        chunks[0] = 0;
        vm.prank(bob);
        poda.submitChunkAttestations(COMMITMENT_1, chunks);
        
        // Issue chunk challenge
        vm.prank(charlie);
        vm.expectEmit(false, true, false, true); // Don't check challengeId
        emit Poda.ChunkChallengeIssued(bytes32(0), COMMITMENT_1, 0, bob);
        bytes32 challengeId = poda.issueChunkChallenge(COMMITMENT_1, 0, bob);
        
        assertNotEq(challengeId, bytes32(0));
    }
    
    function test_IssueChunkChallenge_ProviderDoesntOwnChunk() public {
        vm.prank(alice);
        poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS, KZG_COMMITMENT_1);
        
        vm.prank(charlie);
        vm.expectRevert("Provider doesn't own this chunk");
        poda.issueChunkChallenge(COMMITMENT_1, 0, bob);
    }
    
    function test_RespondToChunkChallenge() public {
        // Setup challenge
        vm.prank(alice);
        poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS, KZG_COMMITMENT_1);
        
        uint16[] memory chunks = new uint16[](1);
        chunks[0] = 0;
        vm.prank(bob);
        poda.submitChunkAttestations(COMMITMENT_1, chunks);
        
        vm.prank(charlie);
        poda.issueChunkChallenge(COMMITMENT_1, 0, bob);
        
        // Respond to challenge
        bytes32 proof = keccak256("valid_chunk_proof");
        vm.prank(bob);
        poda.respondToChunkChallenge(COMMITMENT_1, 0, proof);
        
        // Challenge should be cleared (no longer active)
        // We can verify this by trying to respond again
        vm.prank(bob);
        vm.expectRevert("No active challenge");
        poda.respondToChunkChallenge(COMMITMENT_1, 0, proof);
    }
    
    // function test_SlashProviderChunk() public {
    //     // Setup commitment with chunk
    //     vm.prank(alice);
    //     poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS);
        
    //     uint16[] memory chunks = new uint16[](1);
    //     chunks[0] = 0;
    //     vm.prank(bob);
    //     poda.submitChunkAttestations(COMMITMENT_1, chunks);
        
    //     // Issue challenge
    //     vm.prank(charlie);
    //     poda.issueChunkChallenge(COMMITMENT_1, 0, bob);
        
    //     // Check state before slashing
    //     (, , , , uint16 chunksBefore,) = poda.getCommitmentInfo(COMMITMENT_1);
    //     assertEq(chunksBefore, 1);
    //     assertTrue(poda.isChunkAvailable(COMMITMENT_1, 0));
        
    //     // Slash provider
    //     vm.prank(charlie);
    //     poda.slashProviderChunk(COMMITMENT_1, 0, bob);
        
    //     // Check state after slashing
    //     (, , , , uint16 chunksAfter,) = poda.getCommitmentInfo(COMMITMENT_1);
    //     assertEq(chunksAfter, 0);
    //     assertFalse(poda.isChunkAvailable(COMMITMENT_1, 0));
    //     assertEq(poda.getChunkOwner(COMMITMENT_1, 0), address(0));
        
    //     // Check reputation penalty
    //     (, , , uint16 reputation,) = poda.getProviderInfo(bob);
    //     assertEq(reputation, 9500); // 5% penalty
    // }

    // =============================================================================
    // BATCH OPERATIONS AND VIEW FUNCTION TESTS
    // =============================================================================
    
    function test_BatchChunkAttestations() public {
        // Create commitment
        vm.prank(alice);
        poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS, KZG_COMMITMENT_1);
        
        // Batch attest to multiple chunks
        uint16[] memory chunks = new uint16[](3);
        chunks[0] = 0;
        chunks[1] = 2;
        chunks[2] = 4;
        
        vm.prank(bob);
        poda.submitChunkAttestations(COMMITMENT_1, chunks);
        
        // Check all chunks were recorded
        for (uint i = 0; i < chunks.length; i++) {
            assertTrue(poda.isChunkAvailable(COMMITMENT_1, chunks[i]));
            assertEq(poda.getChunkOwner(COMMITMENT_1, chunks[i]), bob);
        }
        
        // Check count updated
        (Poda.Commitment memory commitment,) = poda.getCommitmentInfo(COMMITMENT_1);
        assertEq(commitment.availableChunks, 3);
    }
    
    function test_BatchChunkAttestations_InvalidSize() public {
        vm.prank(alice);
        poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS, KZG_COMMITMENT_1);
        
        uint16[] memory empty = new uint16[](0);
        vm.prank(alice);
        vm.expectRevert("Invalid chunk count");
        poda.submitChunkAttestations(COMMITMENT_1, empty);
        
        uint16[] memory tooLarge = new uint16[](51);
        vm.prank(alice);
        vm.expectRevert("Invalid chunk count");
        poda.submitChunkAttestations(COMMITMENT_1, tooLarge);
    }
    
    function test_GetAvailableChunks() public {
        vm.prank(alice);
        poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS, KZG_COMMITMENT_1);
        
        uint16[] memory chunks1 = new uint16[](2);
        chunks1[0] = 0;
        chunks1[1] = 2;
        vm.prank(alice);
        poda.submitChunkAttestations(COMMITMENT_1, chunks1);
        
        uint16[] memory chunks2 = new uint16[](1);
        chunks2[0] = 4;
        vm.prank(bob);
        poda.submitChunkAttestations(COMMITMENT_1, chunks2);
        
        uint16[] memory availableChunks = poda.getAvailableChunks(COMMITMENT_1);
        assertEq(availableChunks.length, 3);
        assertEq(availableChunks[0], 0);
        assertEq(availableChunks[1], 2);
        assertEq(availableChunks[2], 4);
    }
    
    function test_GetMultipleCommitmentStatus() public {
        // Create and make one commitment recoverable
        vm.prank(alice);
        poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS, KZG_COMMITMENT_1);
        
        // Add enough chunks to make it recoverable
        uint16[] memory chunks = new uint16[](4);
        chunks[0] = 0;
        chunks[1] = 1;
        chunks[2] = 2;
        chunks[3] = 3;
        vm.prank(alice);
        poda.submitChunkAttestations(COMMITMENT_1, chunks);
        
        // Create but don't make second commitment recoverable
        vm.prank(alice);
        poda.submitCommitment(COMMITMENT_2, NAMESPACE_2, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS, KZG_COMMITMENT_1);
        
        // Check batch status
        bytes32[] memory commitments = new bytes32[](3);
        commitments[0] = COMMITMENT_1;
        commitments[1] = COMMITMENT_2;
        commitments[2] = COMMITMENT_3; // Nonexistent
        
        bool[] memory statuses = poda.getMultipleCommitmentStatus(commitments);
        
        assertTrue(statuses[0]);   // COMMITMENT_1 is recoverable (4/4 chunks)
        assertFalse(statuses[1]);  // COMMITMENT_2 is not recoverable (0/4 chunks)  
        assertFalse(statuses[2]);  // COMMITMENT_3 doesn't exist
    }

    // =============================================================================
    // STORAGE EFFICIENCY TESTS
    // =============================================================================
    
    function test_GetStorageEfficiency() public {
        vm.prank(alice);
        poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS, KZG_COMMITMENT_1);
        
        (uint256 originalSize, uint256 totalStoredSize, uint256 redundancyRatio) = 
            poda.getStorageEfficiency(COMMITMENT_1);
        
        assertEq(originalSize, DATA_SIZE);
        assertEq(totalStoredSize, (DATA_SIZE * TOTAL_CHUNKS) / REQUIRED_CHUNKS); // 1024 * 6 / 4 = 1536
        assertEq(redundancyRatio, (TOTAL_CHUNKS * 10000) / REQUIRED_CHUNKS); // 6 * 10000 / 4 = 15000 (150%)
    }

    // =============================================================================
    // INTEGRATION TESTS
    // =============================================================================
    
    function test_FullReedSolomonWorkflow() public {
        // 1. Create Reed-Solomon commitment (k=4, n=6)
        vm.prank(alice);
        poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS, KZG_COMMITMENT_1);
        
        // 2. Providers attest to different chunks
        uint16[] memory aliceChunks = new uint16[](2);
        aliceChunks[0] = 0;
        aliceChunks[1] = 1;
        vm.prank(alice);
        poda.submitChunkAttestations(COMMITMENT_1, aliceChunks);
        
        uint16[] memory bobChunks = new uint16[](2);
        bobChunks[0] = 2;
        bobChunks[1] = 3;
        vm.prank(bob);
        poda.submitChunkAttestations(COMMITMENT_1, bobChunks);
        
        // 3. Now we have k=4 chunks, should be recoverable
        assertTrue(poda.isCommitmentRecoverable(COMMITMENT_1));
        
        // 4. Add more chunks for redundancy
        uint16[] memory charlieChunks = new uint16[](2);
        charlieChunks[0] = 4;
        charlieChunks[1] = 5;
        vm.prank(charlie);
        poda.submitChunkAttestations(COMMITMENT_1, charlieChunks);

        // 5. Now we have all n=6 chunks
        (Poda.Commitment memory commitment,) = poda.getCommitmentInfo(COMMITMENT_1);
        assertEq(commitment.availableChunks, 6);
        
        // 6. Challenge one chunk
        vm.prank(eve);
        poda.issueChunkChallenge(COMMITMENT_1, 0, alice);
        
        
        // 7. Provider responds successfully
        vm.prank(alice);
        poda.respondToChunkChallenge(COMMITMENT_1, 0, keccak256("proof"));
        
        // 8. Still recoverable and no reputation loss
        assertTrue(poda.isCommitmentRecoverable(COMMITMENT_1));
        Poda.ProviderInfo memory info = poda.getProviderInfo(alice);
        assertEq(info.stakedAmount, 1 ether);
        assertEq(info.challengeCount, 1);
        assertEq(info.challengeSuccessCount, 1);
        assertTrue(info.active);
    }
    
    // function test_FaultTolerance() public {
    //     // Test that we can lose up to (n-k) chunks and still recover
    //     vm.prank(alice);
    //     poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS);
        
    //     // Add all 6 chunks
    //     uint16[] memory allChunks = new uint16[](6);
    //     for (uint16 i = 0; i < 6; i++) {
    //         allChunks[i] = i;
    //     }
    //     vm.prank(alice);
    //     poda.submitChunkAttestations(COMMITMENT_1, allChunks);
        
    //     assertTrue(poda.isCommitmentRecoverable(COMMITMENT_1));
        
    //     // Simulate losing 2 chunks (n-k = 6-4 = 2)
    //     vm.prank(eve);
    //     poda.issueChunkChallenge(COMMITMENT_1, 0, alice);
    //     vm.prank(eve);
    //     poda.slashProviderChunk(COMMITMENT_1, 0, alice);
        
    //     vm.prank(eve);
    //     poda.issueChunkChallenge(COMMITMENT_1, 1, alice);
    //     vm.prank(eve);
    //     poda.slashProviderChunk(COMMITMENT_1, 1, alice);
        
    //     // Should still be recoverable with 4 chunks
    //     assertTrue(poda.isCommitmentRecoverable(COMMITMENT_1));
    //     (, , , , uint16 availableChunks,) = poda.getCommitmentInfo(COMMITMENT_1);
    //     assertEq(availableChunks, 4);
        
    //     // Lose one more chunk - should become unrecoverable
    //     vm.prank(eve);
    //     poda.issueChunkChallenge(COMMITMENT_1, 2, alice);
    //     vm.prank(eve);
    //     poda.slashProviderChunk(COMMITMENT_1, 2, alice);
        
    //     assertFalse(poda.isCommitmentRecoverable(COMMITMENT_1));
    //     (, , , , uint16 finalChunks,) = poda.getCommitmentInfo(COMMITMENT_1);
    //     assertEq(finalChunks, 3); // Below recovery threshold
    // }
    
    // function test_GasUsage() public {
    //     uint256 gasBefore;
    //     uint256 gasAfter;
        
    //     // Register provider gas usage
    //     gasBefore = gasleft();
    //     vm.prank(dave);
    //     poda.registerProvider{value: 1 ether}("Dave", "https://dave.com");
    //     gasAfter = gasleft();
    //     console.log("Register provider gas:", gasBefore - gasAfter);
        
    //     // Submit commitment gas usage  
    //     gasBefore = gasleft();
    //     vm.prank(dave);
    //     poda.submitCommitment(COMMITMENT_1, NAMESPACE_1, DATA_SIZE, TOTAL_CHUNKS, REQUIRED_CHUNKS);
    //     gasAfter = gasleft();
    //     console.log("Submit Reed-Solomon commitment gas:", gasBefore - gasAfter);
        
    //     // Submit chunk attestations gas usage
    //     uint16[] memory chunks = new uint16[](4);
    //     chunks[0] = 0;
    //     chunks[1] = 1;
    //     chunks[2] = 2;
    //     chunks[3] = 3;
        
    //     gasBefore = gasleft();
    //     vm.prank(alice);
    //     poda.submitChunkAttestations(COMMITMENT_1, chunks);
    //     gasAfter = gasleft();
    //     console.log("Submit 4 chunk attestations gas:", gasBefore - gasAfter);
        
    //     // Check chunk availability gas usage
    //     gasBefore = gasleft();
    //     bool available = poda.isChunkAvailable(COMMITMENT_1, 0);
    //     gasAfter = gasleft();
    //     console.log("Check chunk availability gas:", gasBefore - gasAfter);
    //     assertTrue(available);
    // }
}
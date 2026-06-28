// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

import {Test} from "forge-std/Test.sol";
import {MerkleProof} from "@openzeppelin/contracts/utils/cryptography/MerkleProof.sol";

/// @notice Cross-stack parity test: proves that a TS-built proof verifies
/// against on-chain MerkleProof.verify, guaranteeing bit-for-bit parity.
contract MerkleParityTest is Test {
    struct ParityFixture {
        bytes32 root;
        address account;
        uint256 cumulativeAmount;
        bytes32 leaf;
        bytes32[] proof;
    }

    function readFixture() internal view returns (ParityFixture memory) {
        string memory json = vm.readFile("test/fixtures/parity_proof.json");
        bytes memory data = vm.parseJson(json);

        // Parse each field from the JSON
        bytes32 root = vm.parseJsonBytes32(json, ".root");
        address account = vm.parseJsonAddress(json, ".account");
        uint256 cumulativeAmount = vm.parseJsonUint(json, ".cumulativeAmount");
        bytes32 leaf = vm.parseJsonBytes32(json, ".leaf");
        bytes32[] memory proof = vm.parseJsonBytes32Array(json, ".proof");

        return ParityFixture({
            root: root,
            account: account,
            cumulativeAmount: cumulativeAmount,
            leaf: leaf,
            proof: proof
        });
    }

    function testParityProofVerifies() public {
        ParityFixture memory f = readFixture();

        // Verify the proof against the root using OZ MerkleProof
        bool verified = MerkleProof.verify(f.proof, f.root, f.leaf);
        assertTrue(verified, "TS-built proof must verify on-chain");

        // Also verify the leaf matches what we'd compute on-chain
        bytes32 computedLeaf = keccak256(
            bytes.concat(keccak256(abi.encode(f.account, f.cumulativeAmount)))
        );
        assertEq(computedLeaf, f.leaf, "leaf must match on-chain computation");
    }

    function testParityFixtureIsNonEmpty() public {
        ParityFixture memory f = readFixture();
        assertTrue(f.root != bytes32(0), "root must be non-zero");
        assertTrue(f.account != address(0), "account must be non-zero");
        assertTrue(f.proof.length >= 1, "proof must have at least 1 element");
    }
}

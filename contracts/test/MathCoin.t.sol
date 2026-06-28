// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

import {Test, console2} from "forge-std/Test.sol";
import {MathCoin} from "../src/MathCoin.sol";
import {MerkleProof} from "@openzeppelin/contracts/utils/cryptography/MerkleProof.sol";

contract MathCoinTest is Test {
    MathCoin public token;
    address public owner = address(0x100);
    address public user = address(0x200);
    address public relayer = address(0x300);

    function setUp() public {
        vm.prank(owner);
        token = new MathCoin(owner);
    }

    // ---- Helper: build a leaf for the Merkle tree ----
    function makeLeaf(address account, uint256 amount) internal pure returns (bytes32) {
        return keccak256(bytes.concat(keccak256(abi.encode(account, amount))));
    }

    // ---- Helper: build a Merkle tree and return root + proof for a specific account ----
    function buildTree(address[] memory accounts, uint256[] memory amounts, address target)
        internal pure returns (bytes32 root, bytes32[] memory proof)
    {
        require(accounts.length == amounts.length, "length mismatch");
        uint256 n = accounts.length;

        // Build leaves
        bytes32[] memory leaves = new bytes32[](n);
        for (uint256 i = 0; i < n; i++) {
            leaves[i] = makeLeaf(accounts[i], amounts[i]);
        }

        // Build tree bottom-up
        uint256 layers = 0;
        uint256 size = n;
        while (size > 0) { layers++; size >>= 1; }

        bytes32[] memory tree = new bytes32[](2 * n); // simple full binary tree
        for (uint256 i = 0; i < n; i++) {
            tree[n + i] = leaves[i];
        }
        for (uint256 i = n - 1; i > 0; i--) {
            bytes32 left = tree[2 * i];
            bytes32 right = tree[2 * i + 1];
            tree[i] = keccak256(bytes.concat(left < right ? left : right, left < right ? right : left));
        }
        root = tree[1];

        // Find target index and build proof
        int256 targetIdx = -1;
        for (uint256 i = 0; i < n; i++) {
            if (accounts[i] == target) {
                targetIdx = int256(i);
                break;
            }
        }
        require(targetIdx >= 0, "target not found");

        // Build proof: siblings along the path
        uint256 idx = n + uint256(targetIdx);
        bytes32[] memory proofArr = new bytes32[](0);
        // Count layers
        uint256 levelCount = 0;
        for (uint256 sz = n; sz > 1; sz >>= 1) levelCount++;
        proof = new bytes32[](levelCount);
        uint256 pi = 0;
        while (idx > 1) {
            uint256 sibling = idx ^ 1;
            if (sibling < 2 * n) {
                proof[pi++] = tree[sibling];
            }
            idx >>= 1;
        }
        // Trim to actual size
        assembly { mstore(proof, pi) }
    }

    // ---- updateRoot ----

    function testUpdateRootOnlyOwner() public {
        bytes32 newRoot = keccak256("root");
        vm.prank(relayer);
        vm.expectRevert();
        token.updateRoot(newRoot);
    }

    function testUpdateRootOwnerSucceeds() public {
        bytes32 newRoot = keccak256("root");
        vm.prank(owner);
        vm.expectEmit(true, true, true, true);
        emit MathCoin.RootUpdated(newRoot);
        token.updateRoot(newRoot);
        assertEq(token.merkleRoot(), newRoot);
    }

    // ---- claim ----

    function testClaimValidProofMintsDelta() public {
        address[] memory accounts = new address[](2);
        accounts[0] = user;
        accounts[1] = address(0x400);
        uint256[] memory amounts = new uint256[](2);
        amounts[0] = 100;
        amounts[1] = 50;

        (bytes32 root, bytes32[] memory proof) = buildTree(accounts, amounts, user);

        vm.prank(owner);
        token.updateRoot(root);

        vm.expectEmit(true, true, true, true);
        emit MathCoin.Claimed(user, 100);
        token.claim(user, 100, proof);

        assertEq(token.balanceOf(user), 100);
        assertEq(token.claimed(user), 100);
    }

    function testClaimInvalidProofReverts() public {
        // Use 2 leaves so proof is non-empty
        address[] memory accounts = new address[](2);
        accounts[0] = user;
        accounts[1] = address(0x400);
        uint256[] memory amounts = new uint256[](2);
        amounts[0] = 100;
        amounts[1] = 50;

        (bytes32 root, bytes32[] memory proof) = buildTree(accounts, amounts, user);

        vm.prank(owner);
        token.updateRoot(root);

        // Tamper with proof — change the sibling hash
        proof[0] = keccak256("bad");

        vm.expectRevert("MathCoin: invalid proof");
        token.claim(user, 100, proof);
    }

    function testClaimTwiceSameRootReverts() public {
        address[] memory accounts = new address[](1);
        accounts[0] = user;
        uint256[] memory amounts = new uint256[](1);
        amounts[0] = 100;

        (bytes32 root, bytes32[] memory proof) = buildTree(accounts, amounts, user);

        vm.prank(owner);
        token.updateRoot(root);

        token.claim(user, 100, proof);
        assertEq(token.balanceOf(user), 100);

        vm.expectRevert("MathCoin: nothing to claim");
        token.claim(user, 100, proof);
    }

    function testClaimAfterRootUpdateMintsOnlyDelta() public {
        // First distribution: user has 100
        address[] memory accounts1 = new address[](1);
        accounts1[0] = user;
        uint256[] memory amounts1 = new uint256[](1);
        amounts1[0] = 100;
        (bytes32 root1, bytes32[] memory proof1) = buildTree(accounts1, amounts1, user);

        vm.prank(owner);
        token.updateRoot(root1);
        token.claim(user, 100, proof1);
        assertEq(token.balanceOf(user), 100);

        // Second distribution: user now has 250 cumulative (delta = 150)
        address[] memory accounts2 = new address[](1);
        accounts2[0] = user;
        uint256[] memory amounts2 = new uint256[](1);
        amounts2[0] = 250;
        (bytes32 root2, bytes32[] memory proof2) = buildTree(accounts2, amounts2, user);

        vm.prank(owner);
        token.updateRoot(root2);
        token.claim(user, 250, proof2);

        assertEq(token.balanceOf(user), 250);
        assertEq(token.claimed(user), 250);
    }

    function testClaimMintsToAccountNotMsgSender() public {
        address[] memory accounts = new address[](1);
        accounts[0] = user;
        uint256[] memory amounts = new uint256[](1);
        amounts[0] = 100;
        (bytes32 root, bytes32[] memory proof) = buildTree(accounts, amounts, user);

        vm.prank(owner);
        token.updateRoot(root);

        // Relayer submits on behalf of user
        vm.prank(relayer);
        token.claim(user, 100, proof);

        // Tokens go to USER, not relayer
        assertEq(token.balanceOf(user), 100);
        assertEq(token.balanceOf(relayer), 0);
    }

    // ---- Fuzz tests ----

    function testFuzzClaimValidProof(address fuzzUser, uint256 fuzzAmount) public {
        vm.assume(fuzzUser != address(0));
        vm.assume(fuzzAmount > 0 && fuzzAmount < type(uint128).max);

        address[] memory accounts = new address[](1);
        accounts[0] = fuzzUser;
        uint256[] memory amounts = new uint256[](1);
        amounts[0] = fuzzAmount;

        (bytes32 root, bytes32[] memory proof) = buildTree(accounts, amounts, fuzzUser);

        vm.prank(owner);
        token.updateRoot(root);

        token.claim(fuzzUser, fuzzAmount, proof);
        assertEq(token.balanceOf(fuzzUser), fuzzAmount);
        assertEq(token.claimed(fuzzUser), fuzzAmount);
    }

    function testFuzzClaimTwiceReverts(address fuzzUser, uint256 fuzzAmount) public {
        vm.assume(fuzzUser != address(0));
        vm.assume(fuzzAmount > 0 && fuzzAmount < type(uint128).max);

        address[] memory accounts = new address[](1);
        accounts[0] = fuzzUser;
        uint256[] memory amounts = new uint256[](1);
        amounts[0] = fuzzAmount;

        (bytes32 root, bytes32[] memory proof) = buildTree(accounts, amounts, fuzzUser);

        vm.prank(owner);
        token.updateRoot(root);

        token.claim(fuzzUser, fuzzAmount, proof);
        vm.expectRevert("MathCoin: nothing to claim");
        token.claim(fuzzUser, fuzzAmount, proof);
    }
}

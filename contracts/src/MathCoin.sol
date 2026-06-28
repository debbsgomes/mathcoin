// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {Ownable2Step} from "@openzeppelin/contracts/access/Ownable2Step.sol";
import {MerkleProof} from "@openzeppelin/contracts/utils/cryptography/MerkleProof.sol";

/// @title MathCoin — earned off-chain, claimed on-chain via a cumulative Merkle distributor.
contract MathCoin is ERC20, Ownable2Step {
    bytes32 public merkleRoot;
    mapping(address => uint256) public claimed;

    event RootUpdated(bytes32 indexed root);
    event Claimed(address indexed account, uint256 amount);

    constructor(address initialOwner) ERC20("MathCoin", "MATH") Ownable(initialOwner) {}

    /// @notice Publisher posts the latest cumulative-earnings root.
    function updateRoot(bytes32 newRoot) external onlyOwner {
        merkleRoot = newRoot;
        emit RootUpdated(newRoot);
    }

    /// @notice Claim the delta between an account's cumulative entitlement and what it has claimed.
    /// @dev Recipient is the `account` PARAMETER (not msg.sender), so a backend relayer can submit
    ///      on a user's behalf — the tokens still go only to `account` per the proof.
    /// @param account the address that earned (and receives) the tokens, as committed in the root
    /// @param cumulativeAmount total-ever-earned for `account`, as committed in the current root
    /// @param proof OZ Merkle proof for leaf = keccak256(bytes.concat(keccak256(abi.encode(account, amount))))
    function claim(address account, uint256 cumulativeAmount, bytes32[] calldata proof) external {
        bytes32 leaf = keccak256(bytes.concat(keccak256(abi.encode(account, cumulativeAmount))));
        require(MerkleProof.verify(proof, merkleRoot, leaf), "MathCoin: invalid proof");

        uint256 already = claimed[account];
        require(cumulativeAmount > already, "MathCoin: nothing to claim");

        uint256 amount = cumulativeAmount - already;
        claimed[account] = cumulativeAmount; // effects before interaction
        _mint(account, amount);
        emit Claimed(account, amount);
    }
}

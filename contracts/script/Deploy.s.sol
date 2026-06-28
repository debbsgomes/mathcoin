// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

import {Script} from "forge-std/Script.sol";
import {MathCoin} from "../src/MathCoin.sol";

/// @notice Deploy MathCoin to Base Sepolia.
/// Usage:
///   forge script script/Deploy.s.sol --rpc-url base_sepolia --broadcast --verify
///
/// Required env vars:
///   PRIVATE_KEY  — deployer/initial owner private key
///   BASESCAN_API_KEY — for contract verification (optional)
contract Deploy is Script {
    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        address initialOwner = vm.addr(deployerPrivateKey);

        vm.startBroadcast(deployerPrivateKey);

        MathCoin token = new MathCoin(initialOwner);

        vm.stopBroadcast();

        console.log("MathCoin deployed to:", address(token));
        console.log("Initial owner:", initialOwner);
    }
}

# MathCoin — Smart Contract

ERC-20 token with cumulative Merkle distributor, deployed on **Base Sepolia testnet**.

**Contract:** `src/MathCoin.sol`  
**Libraries:** OpenZeppelin ERC20, Ownable2Step, MerkleProof  
**Framework:** Foundry

## Build & Test

```bash
forge build
forge test       # 11 tests (9 unit + 2 fuzz)
```

## Deploy to Base Sepolia

```bash
# Set env vars
export PRIVATE_KEY=<deployer-private-key>
export BASESCAN_API_KEY=<optional-for-verification>

# Deploy
forge script script/Deploy.s.sol --rpc-url base_sepolia --broadcast --verify
```

After deploy, set the contract address in `api/.env`:
```
CONTRACT_ADDRESS=0x...
CHAIN_NAME=base_sepolia
CHAIN_ID=84532
EXPLORER_URL=https://sepolia.basescan.org
```

## Testnet Only

This contract is intended for **Base Sepolia testnet only**.
There is no mainnet deployment. The MATH token has no monetary value.

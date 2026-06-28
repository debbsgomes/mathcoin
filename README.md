# MathCoin — Play-to-Earn Math Game

A full-stack play-to-earn math game with a real on-chain settlement layer deployed on
**Base Sepolia testnet**. The MATH token is a deliberately valueless ERC-20 used to
demonstrate the full architecture: off-chain gameplay → Merkle distribution → on-chain
claiming via a cumulative Merkle distributor.

## Architecture

```
┌──────────┐     ┌───────────┐     ┌────────────┐     ┌───────────────┐
│  Web UI  │────▶│  Rust API │────▶│ Publisher  │────▶│ Base Sepolia  │
│ (Vue 3)  │     │  (Axum)   │     │  (TS)      │     │ (MathCoin.sol)│
└──────────┘     └───────────┘     └────────────┘     └───────────────┘
                       │                                      │
                       ▼                                      ▼
                ┌──────────┐                          ┌───────────┐
                │PostgreSQL│                          │ Chain     │
                │(earnings)│                          │ Indexer   │
                └──────────┘                          │ (Rust)    │
                                                     └───────────┘
```

1. **Web UI** (Vue 3 + TypeScript) — Math challenge game, Supabase auth, wallet
2. **Rust API** (Axum + sqlx) — Generates challenges at adaptive difficulty, validates answers, awards off-chain tokens in PostgreSQL
3. **Publisher** (TypeScript) — Snapshots cumulative earnings, builds OpenZeppelin Merkle trees, persists proofs as JSONB
4. **MathCoin.sol** (Solidity, Foundry) — ERC-20 token with cumulative Merkle distributor (`updateRoot` + `claim`). Deployed on **Base Sepolia testnet**
5. **Rust On-Chain Adapter** — TxSubmitter (nonce management + gap recovery), event indexer (cursor + idempotent replay), claim relay

## On-Chain Layer (Testnet Only)

The on-chain layer is a **real, deployed ERC-20** on Base Sepolia. It is a **demonstration**,
not a financial product:

- **Contract:** `MathCoin.sol` — ERC-20 + `Ownable2Step` + `MerkleProof`
- **Network:** Base Sepolia (chain ID `84532`)
- **Token:** MATH has **no monetary value** and is never intended to acquire any
- **Explorer:** [sepolia.basescan.org](https://sepolia.basescan.org)
- **No mainnet deployment** exists or is planned

The on-chain infrastructure (relayer, indexer, TxSubmitter) exercises real blockchain
operations — nonce management, gas estimation, confirmation polling, event indexing —
but against a testnet where mistakes carry no financial risk.

## Quick Start

### Prerequisites

- Rust toolchain (stable)
- Node.js >= 18
- PostgreSQL
- Foundry (for contracts)

### API

```bash
cd api
cp .env.example .env    # edit with real values
cargo run
```

### Web Frontend

```bash
cd web
cp .env.example .env    # edit with real values
npm install
npm run dev
```

### Contracts

```bash
cd contracts
forge install            # install OZ + forge-std dependencies
forge test               # 11 tests (incl. 2 fuzz)
```

### Publisher

```bash
cd publisher
npm install
DATABASE_URL=postgres://... npx tsx src/cli.ts
```

### Deploy to Base Sepolia

```bash
cd contracts
source .env              # sets PRIVATE_KEY, BASESCAN_API_KEY
forge script script/Deploy.s.sol --rpc-url base_sepolia --broadcast --verify
```

## Tests

| Suite | Framework | Tests |
|-------|-----------|-------|
| Rust API handler tests | cargo test | 28 tests (handler + auth + chain + challenge) |
| Rust concurrency tests | cargo test | 7 tests (100-racer double-credit proof) |
| Foundry contract tests | forge test | 11 tests (9 unit + 2 fuzz) |
| Publisher tests | vitest | 10 tests (Merkle tree + publish pipeline) |
| Frontend tests | vitest | Game + Audit components |

## Security

See [docs/SECURITY.md](docs/SECURITY.md) for the OWASP Top 10:2025 compliance matrix,
secrets management table, and deferred security items.

## License

MIT

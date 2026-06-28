# MathCoin — Play-to-Earn Math Game

🚧 Status: Active Development (Architecture & Infrastructure Proof of Concept)

A full-stack play-to-earn math game with a real on-chain settlement layer deployed on
**Base Sepolia testnet**. The MATH token is a deliberately valueless ERC-20 used to
demonstrate the full architecture: off-chain gameplay → Merkle distribution → on-chain
claiming via a cumulative Merkle distributor.

> **Testnet only.** No mainnet deployment exists or is planned.
> MATH has no monetary value and is never intended to acquire any.

---

## Getting Started (Run Locally)

The core game (auth + gameplay + in-app wallet) runs locally with two external dependencies:
**Supabase** (for email/Google login + Postgres) and **Node.js + Rust**.

### Prerequisites

- **Docker** + Docker Compose
- **Node.js** >= 18 and npm
- **Rust** (stable) + `cargo`
- **sqlx-cli**: `cargo install sqlx-cli --no-default-features --features native-tls,postgres`
- **Supabase CLI**: `npm i -g supabase` (or [official installer](https://supabase.com/docs/guides/cli))
- (Optional) **Foundry** (`forge`, `cast`) — only needed for the on-chain layer

### Quick Setup (one command)

```bash
git clone https://github.com/debbsgomes/mathcoin.git
cd mathcoin
bash scripts/setup.sh
```

### Step-by-Step

**1. Start local Supabase (Auth + Postgres)**

```bash
supabase start
```

This boots a full local Supabase stack in Docker. Run `supabase status` to see the
local URLs and keys. Copy the output values into your `.env` file.

**2. Configure environment**

```bash
cp .env.example .env               # root .env is the single source of truth
cp api/.env.example api/.env        # mirror for running the API standalone
```

The **root `.env.example`** is the single source of truth. `api/.env.example` is a
subset mirror for when you run `cd api && cargo run` directly (the API binary loads
`api/.env` from its working directory).

Fill in `.env` with the values from `supabase status`:
- `DATABASE_URL` ← DB URL from `supabase status`
- `JWKS_URL` ← `http://127.0.0.1:54321/auth/v1/.well-known/jwks.json`
- `JWT_ISS` ← `http://127.0.0.1:54321/auth/v1`
- Leave the on-chain vars empty (on-chain is disabled by default).

Also configure the frontend (`web/.env`):
- `VITE_SUPABASE_URL` ← API URL from `supabase status`
- `VITE_SUPABASE_ANON_KEY` ← anon key from `supabase status`
- `VITE_API_URL=http://127.0.0.1:3000`

**3. Run database migrations**

```bash
cd api && sqlx migrate run && cd ..
```

**4. Start the API**

```bash
cd api && cargo run
```

The API starts on `http://127.0.0.1:3000`. Verify with `curl http://127.0.0.1:3000/api/health`.

**5. Start the frontend**

```bash
cd web && npm install && npm run dev
```

Open `http://localhost:5173`. Sign in with email/password or Google.

### Where's the blockchain?

The core game works entirely off-chain — no wallet, no gas, no RPC needed.
The on-chain layer is **optional**. See [On-Chain Layer (Optional)](#on-chain-layer-optional) below.

---

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

1. **Web UI** (Vue 3 + TypeScript) — Math challenge game, Supabase auth (email/Google), in-app wallet
2. **Rust API** (Axum + sqlx) — Generates challenges at adaptive difficulty, validates answers server-side, awards off-chain coins in PostgreSQL
3. **Publisher** (TypeScript) — Snapshots cumulative earnings, builds OpenZeppelin Merkle trees, persists proofs as JSONB
4. **MathCoin.sol** (Solidity, Foundry) — ERC-20 token with cumulative Merkle distributor (`updateRoot` + `claim`). Deployed on Base Sepolia testnet
5. **Rust On-Chain Adapter** — TxSubmitter (nonce management + gap recovery), event indexer (cursor + idempotent replay), claim relay

---

## On-Chain Layer (Optional)

The on-chain layer is a **real, deployed ERC-20** on Base Sepolia. It is a **demonstration**,
not a financial product. **It is NOT required to run or evaluate the project.**

### Enable on-chain features

1. **Deploy the contract to Base Sepolia:**

```bash
cd contracts
forge install                        # install OZ + forge-std dependencies
source .env                          # sets PRIVATE_KEY, BASESCAN_API_KEY
forge script script/Deploy.s.sol --rpc-url base_sepolia --broadcast --verify
```

2. **Set the on-chain env vars in `.env`:**

```
CONTRACT_ADDRESS=0x...               # the deployed contract address
CHAIN_NAME=base_sepolia
CHAIN_ID=84532
EXPLORER_URL=https://sepolia.basescan.org
BASE_RPC_URL=https://sepolia.base.org
RELAYER_PRIVATE_KEY=0x...            # testnet-only key
```

3. **Restart the API.** On-chain endpoints are now available.

### On-chain features

- `POST /api/claim-address` — Opt in by setting a destination address (works even without on-chain config)
- `GET /api/claim-data` — Returns Merkle proof for on-chain claiming
- `GET /api/claim-relay` — Submits claim transaction (relayed, gasless for user)
- `GET /api/audit` — Public proof-of-reserves view

### Run the publisher

```bash
cd publisher && npm install
DATABASE_URL=postgres://... npx tsx src/cli.ts
```

The publisher snapshots earnings, builds the Merkle tree, and persists the distribution.
The Rust adapter handles the on-chain `updateRoot` transaction.

---

## Local Database Options

The project supports **two** ways to run Postgres locally.
**Use ONE or the other — never both at once** (they conflict on port 5432).

### Recommended: Supabase CLI (`supabase start`)

This is the default path for clone-and-run. It provides **both Postgres and Auth**
in one command — everything the app needs to function.

```bash
supabase start        # boots Postgres + Auth + API in Docker
supabase status       # prints URLs and keys → copy into .env
```

This is what the Getting Started steps above assume.

### Alternative: Standalone Postgres (docker-compose)

For **CI pipelines**, or if you already use **Supabase cloud** (managed) and only
need a local Postgres for development. This starts ONLY Postgres — you must
point `DATABASE_URL` at it and configure `JWKS_URL`/`JWT_ISS` to Supabase cloud.

```bash
cp .env.example .env  # edit DATABASE_URL to point at the compose DB
docker compose up api  # starts API + Postgres
```

> **Do not run both** `supabase start` and `docker compose up` at the same time —
> they both bind port 5432 and will conflict.

The Dockerfile uses `SQLX_OFFLINE=true` for builds without a live database.
The `.sqlx/` directory is intentionally committed (currently empty — this project uses
runtime-checked sqlx queries, not `query!` macros). If you adopt `query!` macros later,
run `cargo sqlx prepare` to populate the cache so the offline Docker build keeps working.

---

## Tests

| Suite | Framework | Command | Tests |
|-------|-----------|---------|-------|
| Rust handler tests | cargo test | `cargo test --test handler_tests` | 28 |
| Rust chain/adapter tests | cargo test | `cargo test --test chain_tests` | 8 |
| Rust auth verifier tests | cargo test | `cargo test --test auth_verifier_tests` | 9 |
| Rust challenge tests | cargo test | `cargo test --test challenge_tests` | 9 |
| Rust concurrency tests | cargo test | `cargo test --test concurrency_tests` | 7 |
| Foundry contract tests | forge test | `forge test` | 11 (incl. 2 fuzz) |
| Publisher tests | vitest | `npm test` | 10 |
| Frontend tests | vitest | `npm test` | 2 |

---

## Troubleshooting

### "sqlx: pool timed out" in tests
The concurrency tests (100 racers) can exhaust the connection pool. Run them with `--test-threads=1`.

### "failed to bind" on startup
The default bind is `127.0.0.1:3000`. In Docker/cloud hosts, set `BIND_ADDRESS=0.0.0.0:3000` or `PORT=3000`.

### CORS errors in the browser
Ensure `FRONTEND_ORIGIN` matches exactly the frontend URL (including protocol and port).
For local dev: `FRONTEND_ORIGIN=http://localhost:5173`.

### Supabase CLI not running
Run `supabase start`. If it fails, check Docker is running. The `supabase status` command
prints all the URLs and keys you need.

### Missing required env var
The API fails fast at startup if `DATABASE_URL`, `JWT_ISS`, `JWT_AUD`, or `JWKS_URL` are missing.
On-chain vars (`CONTRACT_ADDRESS`, etc.) are optional — the core runs without them.

### "on-chain disabled" (503) on claim endpoints
`CONTRACT_ADDRESS` is not set in `.env`. This is expected for local dev. Set it to enable.

### sqlx offline builds in Docker
The Dockerfile sets `SQLX_OFFLINE=true`. The `.sqlx/` cache directory is committed and
intentionally kept (with a `.gitkeep`). It is currently empty because the project uses
runtime-checked `sqlx::query`/`sqlx::query_as` function variants. If you adopt the
`sqlx::query!` / `sqlx::query_as!` compile-time-checked macros, run `cargo sqlx prepare`
to populate the cache so the Docker build keeps working.

### Two Postgres instances (port conflict)
If you see "port 5432 already in use", you have both `supabase start` and
`docker compose up` running. Stop one: `supabase stop` or `docker compose down`.

### Google sign-in redirects to wrong URL
In Supabase dashboard → Authentication → URL Configuration, set Site URL to your frontend URL
and add it to Redirect URLs. For local dev: `http://localhost:5173`.

---

## Security

See [docs/SECURITY.md](docs/SECURITY.md) for the OWASP Top 10:2025 compliance matrix,
secrets management table, and deferred security items.

## License

MIT — see [LICENSE](LICENSE).

# Security — MathCoin

## OWASP Top 10:2025 Status

| Category | Phase 1 | Notes |
|---|---|---|
| A01 Broken Access Control | ✅ | Identity from JWT only. CORS restricted. Mint validates challenge ownership (user_id). |
| A02 Security Misconfiguration | ✅ | `.env` gitignored. Security headers active. TraceLayer logging. |
| A03 Supply Chain | ✅ | Deps versioned. npm audit: 0 vulns (both web + publisher). |
| A04 Cryptographic Failures | ✅ | ES256/P-256 JWT. HTTPS enforced for JWKS. |
| A05 Injection | ✅ | Parameterized queries (sqlx). Vue auto-escapes. |
| A06 Insecure Design | ✅ | Threat model in ADD v3. Trust boundaries explicit. |
| A07 Authentication Failures | ✅ | Supabase-managed auth. JWT validated (iss/aud/exp/kid/sig). |
| A08 Software/Data Integrity | ✅ | JWT crypto-verified. No deserialization of untrusted data. |
| A09 Logging & Monitoring | ✅ | TraceLayer logs requests. tracing::error/warn for failures. No PII in spans. |
| A10 Exceptional Conditions | ✅ | Generic 500. No stack traces. Fail-closed on auth + DB errors. |

## Deferred Security Items

The on-chain layer (ERC-20 + Merkle distributor + relayer + indexer) runs on **Base Sepolia testnet only**.
There is no mainnet deployment. These items apply to the testnet environment.

| Item | Status | Rationale |
|---|---|---|
| Relayer private key in env var | Deployed | Key stored as `RELAYER_PRIVATE_KEY` env var. KMS not needed for testnet. |
| Multisig contract owner | Not needed | `Ownable2Step` implemented. Single-key owner acceptable for testnet. |
| CSP headers on frontend | Testnet-only | Handled by Vercel/Netlify when deployed. |
| HSTS preload | Testnet-only | Requires valid TLS cert on public domain. |
| JWT stored in `localStorage` | Testnet-only | Deliberately valueless token. No migration needed. |
| Panic handler middleware | Testnet-only | TraceLayer logs all requests. Acceptable for testnet demo. |
| Install `cargo-audit` in CI | To do | Not installed locally. Should run periodically. |
| Claim relay wiring (TxSubmitter in AppState) | To do | Route registered but returns placeholder tx_hash on testnet. |

## Secrets Management

Secrets are loaded from environment variables (`.env` in development, platform env vars in production).
The `.env` file is **never** committed to git (see `.gitignore`).
All on-chain values target **Base Sepolia testnet**.

| Secret | Env Var | Location | Notes |
|---|---|---|---|
| Database URL | `DATABASE_URL` | api/.env | |
| Supabase JWKS URL | `JWKS_URL` | api/.env | |
| Supabase JWT issuer | `JWT_ISS` | api/.env | |
| Supabase JWT audience | `JWT_AUD` | api/.env | |
| Supabase URL | `VITE_SUPABASE_URL` | web/.env | |
| Supabase anon key | `VITE_SUPABASE_ANON_KEY` | web/.env | |
| Deployer private key | `PRIVATE_KEY` | contracts/.env | Foundry deploy script |
| Relayer private key | `RELAYER_PRIVATE_KEY` | api/.env | TxSubmitter for updateRoot + claim relay (testnet only) |
| Base Sepolia RPC URL | `BASE_RPC_URL` | api/.env | e.g. `https://sepolia.base.org` |
| Contract address | `CONTRACT_ADDRESS` | api/.env | Deployed MathCoin on Base Sepolia |
| Chain name | `CHAIN_NAME` | api/.env | `base_sepolia` |
| Chain ID | `CHAIN_ID` | api/.env | `84532` |
| Explorer URL | `EXPLORER_URL` | api/.env | `https://sepolia.basescan.org` |

## OWASP Review — Phase 5 Findings & Fixes

| # | Finding | Category | Severity | Status |
|---|---------|----------|----------|--------|
| 1 | Mint endpoint não verificava `user_id` do challenge (cross-account challenge theft) | A01 | HIGH | ✅ Fixed — query inclui `AND user_id = $2` |
| 2 | Rotas claim-address / claim-data / claim-relay não registradas no router | A01 | MEDIUM | ✅ Fixed — registradas em main.rs |
| 3 | Resposta do usuário (`answer`) logada no tracing span do mint | A09 | LOW | ✅ Fixed — campo removido do span |
| 4 | Endpoints stats/audit faziam fail-open (retornavam 0 em erro de DB) | A10 | LOW | ✅ Fixed — propagam `AppError::Internal` |
| 5 | Vulnerabilidades npm moderadas no publisher (uuid, ws) | A03 | LOW | ✅ Fixed — overrides em package.json |

## Reporting a Vulnerability

MathCoin is a portfolio demonstration running on **Base Sepolia testnet only**.
The MATH token is deliberately valueless — it is not deployed to mainnet and has no monetary value.
There is no mainnet deployment planned.

Security feedback is still welcome — open an issue or contact the maintainer.

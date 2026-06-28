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

## Deferred Security Items (by architecture phase)

| Item | Target Phase | Rationale |
|---|---|---|
| Relayer private key in KMS | Phase 6 | No relayer yet. Key stored as env var in Phase 5. |
| Multisig contract owner | Phase 6 (stretch) | Single key acceptable for portfolio. |
| CSP headers on frontend | Production deploy | Handled by Vercel/Netlify when deployed. |
| HSTS preload | Production deploy | Requires valid TLS cert on public domain. |
| JWT stored in `localStorage` | If token ever gains value | Currently valueless game token. If MATH ever acquires monetary value, migrate JWT to `httpOnly` + `Secure` + `SameSite=Strict` cookie to prevent XSS-based token theft. |
| Panic handler middleware | Production deploy | TraceLayer logs all requests. Handler panic → dropped connection (acceptable for portfolio). |
| Install `cargo-audit` in CI | Phase 6 | Not installed locally. Should run periodically. |

## Secrets Management

Secrets are loaded from environment variables (`.env` in development, platform env vars in production).
The `.env` file is **never** committed to git (see `.gitignore`).

| Secret | Env Var | Location |
|---|---|---|
| Database URL | `DATABASE_URL` | api/.env |
| Supabase JWKS URL | `JWKS_URL` | api/.env |
| Supabase JWT issuer | `JWT_ISS` | api/.env |
| Supabase JWT audience | `JWT_AUD` | api/.env |
| Supabase URL | `VITE_SUPABASE_URL` | web/.env |
| Supabase anon key | `VITE_SUPABASE_ANON_KEY` | web/.env |
| Deployer private key | `PRIVATE_KEY` | contracts/.env (Foundry script) |
| Contract address | `CONTRACT_ADDRESS` | api/.env |
| Chain name | `CHAIN_NAME` | api/.env |
| Explorer URL | `EXPLORER_URL` | api/.env |

## OWASP Review — Phase 5 Findings & Fixes

| # | Finding | Category | Severity | Status |
|---|---------|----------|----------|--------|
| 1 | Mint endpoint não verificava `user_id` do challenge (cross-account challenge theft) | A01 | HIGH | ✅ Fixed — query inclui `AND user_id = $2` |
| 2 | Rotas claim-address / claim-data / claim-relay não registradas no router | A01 | MEDIUM | ✅ Fixed — registradas em main.rs |
| 3 | Resposta do usuário (`answer`) logada no tracing span do mint | A09 | LOW | ✅ Fixed — campo removido do span |
| 4 | Endpoints stats/audit faziam fail-open (retornavam 0 em erro de DB) | A10 | LOW | ✅ Fixed — propagam `AppError::Internal` |
| 5 | Vulnerabilidades npm moderadas no publisher (uuid, ws) | A03 | LOW | ✅ Fixed — overrides em package.json |

## Reporting a Vulnerability

This is a portfolio project with a deliberately valueless token.
Security feedback is still welcome — open an issue or contact the maintainer.

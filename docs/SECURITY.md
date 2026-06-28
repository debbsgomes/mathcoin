# Security — MathCoin

## OWASP Top 10:2025 Status

| Category | Phase 1 | Notes |
|---|---|---|
| A01 Broken Access Control | ✅ | Identity from JWT only. CORS restricted. |
| A02 Security Misconfiguration | ✅ | `.env` gitignored. Security headers active. TraceLayer logging. |
| A03 Supply Chain | ✅ | Deps versioned. npm audit: 0 vulns. |
| A04 Cryptographic Failures | ✅ | ES256/P-256 JWT. HTTPS enforced for JWKS. |
| A05 Injection | ✅ | Parameterized queries (sqlx). Vue auto-escapes. |
| A06 Insecure Design | ✅ | Threat model in ADD v3. Trust boundaries explicit. |
| A07 Authentication Failures | ✅ | Supabase-managed auth. JWT validated (iss/aud/exp/kid/sig). |
| A08 Software/Data Integrity | ✅ | JWT crypto-verified. No deserialization of untrusted data. |
| A09 Logging & Monitoring | ✅ | TraceLayer logs requests. tracing::error/warn for failures. |
| A10 Exceptional Conditions | ✅ | Generic 500. No stack traces. Fail-closed on auth. |

## Deferred Security Items (by architecture phase)

| Item | Target Phase | Rationale |
|---|---|---|
| Rate limiting (`tower_governor`) | Phase 4 | Token has no value → abuse is spam, not theft. Difficulty retarget also dampens bots. |
| Full `AppError` taxonomy | Phase 4 | Current 401/500 covers Phase 1-3. Domain errors (409, 422, 410) added in Phase 2. |
| Panic handler middleware | Phase 4 | TraceLayer logs all requests. Handler panic → dropped connection (acceptable for portfolio). |
| `onlyOwner` on `updateRoot()` | Phase 5 | Smart contract not deployed yet. |
| Relayer private key in KMS | Phase 6 | No relayer yet. Key stored as env var in Phase 5. |
| Multisig contract owner | Phase 6 (stretch) | Single key acceptable for portfolio. |
| CSP headers on frontend | Production deploy | Handled by Vercel/Netlify when deployed. |
| HSTS preload | Production deploy | Requires valid TLS cert on public domain. |

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

## Reporting a Vulnerability

This is a portfolio project with a deliberately valueless token.
Security feedback is still welcome — open an issue or contact the maintainer.

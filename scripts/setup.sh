#!/usr/bin/env bash
set -euo pipefail

echo "=== MathCoin — Local Setup ==="
echo ""

# ---- Step 1: .env ----
if [ ! -f .env ]; then
  echo "[1/5] Copying .env.example → .env (edit with real values)"
  cp .env.example .env
  echo "       Done. Edit .env now or after supabase start fills in values."
else
  echo "[1/5] .env already exists — skipping."
fi

# ---- Step 2: Supabase local ----
echo ""
echo "[2/5] Starting local Supabase (Auth + Postgres)..."
if command -v supabase &>/dev/null; then
  supabase start 2>/dev/null || echo "       Supabase already running or failed. Run 'supabase status' to check."
  echo ""
  echo "       Values to copy into your .env:"
  echo "       Run: supabase status"
  echo "       Copy: API URL       → JWKS_URL / JWT_ISS"
  echo "       Copy: DB URL        → DATABASE_URL"
  echo "       Copy: anon key      → VITE_SUPABASE_ANON_KEY (web/.env)"
  echo "       Copy: service_role   → (not needed)"
else
  echo "       Supabase CLI not installed. Install: npm i -g supabase"
  echo "       Or point DATABASE_URL at any Postgres and use JWT_VERIFICATION_MODE=shared_secret."
fi

# ---- Step 3: API dependencies ----
echo ""
echo "[3/5] Building API (Rust)..."
cd api
cargo build 2>&1 | tail -1
cd ..

# ---- Step 4: Frontend dependencies ----
echo ""
echo "[4/5] Installing frontend dependencies..."
cd web
npm install --silent 2>&1 | tail -1
cd ..

# ---- Step 5: Publisher dependencies ----
echo ""
echo "[5/5] Installing publisher dependencies..."
cd publisher
npm install --silent 2>&1 | tail -1
cd ..

echo ""
echo "=== Setup complete ==="
echo ""
echo "Next steps:"
echo "  1. Fill in .env with values from 'supabase status'"
echo "  2. Run migrations:  cd api && sqlx migrate run"
echo "  3. Start API:       cd api && cargo run"
echo "  4. Start frontend:  cd web && npm run dev"
echo "  5. Open:            http://localhost:5173"
echo ""
echo "On-chain layer (optional): set CONTRACT_ADDRESS in .env to enable."
echo "See README.md for full instructions."

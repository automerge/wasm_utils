#!/usr/bin/env bash
# Build the fixture crate to generate a `.d.ts`, then type-check the TS
# consumer against it with `tsc --noEmit`.
#
# Usage: tests/wasm_ts_fixture/check-ts.sh
set -euo pipefail

FIXTURE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "==> Building fixture with wasm-pack (target nodejs)"
wasm-pack build --target nodejs --dev --out-dir pkg "$FIXTURE_DIR"

echo "==> Type-checking generated .d.ts with tsc --noEmit"
# Prefer a tsc on PATH (the Nix dev shell provides one); fall back to npx.
if command -v tsc >/dev/null 2>&1; then
  tsc --noEmit --project "$FIXTURE_DIR/ts/tsconfig.json"
else
  npx --yes -p typescript@5 tsc --noEmit --project "$FIXTURE_DIR/ts/tsconfig.json"
fi

echo "==> TypeScript acceptance check passed"

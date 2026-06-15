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
npx --yes -p typescript@5 tsc --noEmit --project "$FIXTURE_DIR/ts/tsconfig.json"

echo "==> TypeScript acceptance check passed"

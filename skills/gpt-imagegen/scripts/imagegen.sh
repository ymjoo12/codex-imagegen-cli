#!/usr/bin/env bash
# Resolve the codex-imagegen binary across machines and exec it with all args.
# Resolution order:
#   1. $CODEX_IMAGEGEN_BIN (explicit override — must point at an executable)
#   2. codex-imagegen on PATH
#   3. npx codex-imagegen-cli (npm package; downloads binary on first run)
#   4. bunx codex-imagegen-cli
# Exits 127 with installation guidance when none are available.

set -euo pipefail

if [ -n "${CODEX_IMAGEGEN_BIN:-}" ]; then
  if [ -x "$CODEX_IMAGEGEN_BIN" ]; then
    exec "$CODEX_IMAGEGEN_BIN" "$@"
  fi
  echo "CODEX_IMAGEGEN_BIN is set but not executable: $CODEX_IMAGEGEN_BIN" >&2
  exit 127
fi

if command -v codex-imagegen >/dev/null 2>&1; then
  exec codex-imagegen "$@"
fi

if command -v npx >/dev/null 2>&1; then
  exec npx -y codex-imagegen-cli "$@"
fi

if command -v bunx >/dev/null 2>&1; then
  exec bunx codex-imagegen-cli "$@"
fi

cat >&2 <<'EOF'
codex-imagegen binary not found.

Install one of these:
  - npm install -g codex-imagegen-cli
  - bun install -g codex-imagegen-cli
  - Download a release: https://github.com/ymjoo12/codex-imagegen-cli/releases
  - Build from source:  cargo build --release  (in codex-imagegen-cli)

Or set CODEX_IMAGEGEN_BIN to an absolute binary path.
EOF
exit 127

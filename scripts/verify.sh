#!/bin/zsh
# 單一驗證入口，CI 與本機共用。加 --smoke 另跑原生 smoke（需要視窗伺服器與網路）。
set -euo pipefail
cd "${0:A:h:h}"
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
node scripts/test-shortcut-ui.cjs
if [[ "${1:-}" == "--smoke" ]]; then
  # smoke 會改寫設定，必須使用獨立目錄。
  SMOKE_DIR="$(mktemp -d)"
  trap 'rm -rf "$SMOKE_DIR"' EXIT
  cargo run --locked -- --smoke-test --data-dir "$SMOKE_DIR"
fi
print "驗證通過"

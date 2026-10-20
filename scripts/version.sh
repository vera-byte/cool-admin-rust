#!/usr/bin/env bash
# 打印/统一 workspace 版本号
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${1:-}"

if [[ -z "$VERSION" ]]; then
  grep -m1 '^version = ' "$ROOT/Cargo.toml"
  exit 0
fi

echo "set workspace version -> $VERSION"
if [[ "$(uname)" == "Darwin" ]]; then
  sed -i '' "s/^version = .*/version = \"$VERSION\"/" "$ROOT/Cargo.toml"
else
  sed -i "s/^version = .*/version = \"$VERSION\"/" "$ROOT/Cargo.toml"
fi
echo "done"

#!/usr/bin/env bash

set -euo pipefail

project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

for command in cargo npm; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "error: required command '$command' was not found in PATH" >&2
    exit 1
  fi
done

if ! cargo tauri --version >/dev/null 2>&1; then
  echo "error: Tauri CLI 2 is required; install it with:" >&2
  echo "  cargo install tauri-cli --version '^2' --locked" >&2
  exit 1
fi

echo "Installing locked frontend dependencies..."
npm --prefix "$project_root/frontend" ci

echo "Building Perfboard Designer..."
cd -- "$project_root"
cargo tauri build --no-bundle

echo "Build complete: $project_root/target/release/perfboard-designer"

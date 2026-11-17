#!/usr/bin/env bash
set -euo pipefail

# Path is relative to this script; do not assume a user's home directory.
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ -x target/release/daw-ui ]]; then
  exec target/release/daw-ui "$@"
elif [[ -x target/debug/daw-ui ]]; then
  exec target/debug/daw-ui "$@"
else
  echo "Slate Studio is not built. Run: cargo build --release -p daw-ui" >&2
  exit 1
fi

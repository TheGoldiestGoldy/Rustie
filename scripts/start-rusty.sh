#!/usr/bin/env bash
# Stable entry point. Never silently select a stale target/release/daw-ui.
set -euo pipefail
INSTALLED="$HOME/.local/bin/rusty"
if [[ ! -x "$INSTALLED" ]]; then
  echo "Rusty is not installed yet. Run scripts/install-rusty-local.sh from the integrated repository." >&2
  exit 1
fi
exec "$INSTALLED" "$@"

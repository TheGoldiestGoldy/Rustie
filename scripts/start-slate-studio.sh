#!/usr/bin/env bash
# Compatibility for old scripts/bookmarks, now always uses the installed Rusty.
set -euo pipefail
exec "$(dirname "$(realpath "$0")")/start-rusty.sh" "$@"

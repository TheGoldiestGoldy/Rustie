#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REVIEW_DIR="${RUSTIE_UX_REVIEW_DIR:-$HOME/Music/Rustie UX Review}"
python3 "$ROOT/scripts/generate-ux-review.py" "$REVIEW_DIR"
PROJECT="$REVIEW_DIR/Rustie UX Review.slate.json"
"$ROOT/scripts/start-slate-studio.sh" --check-project "$PROJECT" || {
  echo "Review preflight failed; no existing project was overwritten" >&2
  exit 1
}
exec "$ROOT/scripts/start-slate-studio.sh" --project "$PROJECT" "$@"

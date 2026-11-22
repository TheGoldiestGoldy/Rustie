#!/usr/bin/env bash
# Install a versioned Rusty UI/worker pair; never launch old master from the app menu.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"
if [[ -n "$(git status --porcelain)" ]]; then
  echo "Refusing to install uncommitted Rusty source. Commit and pass QA first." >&2
  exit 2
fi
SHA="$(git rev-parse --short=12 HEAD)"
# This value is a declared Cargo build input, not a post-build guess.
# Cargo must rebuild whenever the integrated source revision advances.
export RUSTY_BUILD_REVISION="$SHA"
TARGET_DIR="${RUSTY_TARGET_DIR:-$HOME/.cache/rustie-target-integration}"
export CARGO_TARGET_DIR="$TARGET_DIR"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
cargo build --release -p daw-ui -p daw-plugin-host
BIN="$TARGET_DIR/release"
ACTUAL="$("$BIN/daw-ui" --version)"
if [[ "$ACTUAL" != *"($SHA)"* ]]; then
  echo "Build identity mismatch: expected $SHA, binary reports $ACTUAL" >&2
  exit 3
fi
PREFIX="$HOME/.local/opt/rusty"
VERSIONS="$PREFIX/versions"
DEST="$VERSIONS/$SHA"
mkdir -p "$VERSIONS" "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.local/share/rusty/legacy-desktop"
if [[ ! -d "$DEST" ]]; then
  STAGE="$(mktemp -d "$VERSIONS/.staging-$SHA.XXXXXXXX")"
  trap 'rm -rf -- "$STAGE"' EXIT
  install -m 0755 "$BIN/daw-ui" "$STAGE/rusty"
  install -m 0755 "$BIN/daw-plugin-host" "$STAGE/rusty-plugin-host"
  (cd "$STAGE" && sha256sum rusty rusty-plugin-host > SHA256SUMS)
  test "$("$STAGE/rusty" --version)" = "$ACTUAL"
  mv -- "$STAGE" "$DEST"
  trap - EXIT
else
  # A source revision is immutable; never overwrite an existing build silently.
  (cd "$DEST" && sha256sum --check --status SHA256SUMS)
  test "$("$DEST/rusty" --version)" = "$ACTUAL"
fi
LAUNCHER="$HOME/.local/bin/rusty"
LAUNCHER_TMP="$(mktemp "$HOME/.local/bin/.rusty.XXXXXXXX")"
cat > "$LAUNCHER_TMP" <<'LAUNCHER_EOF'
#!/usr/bin/env bash
set -euo pipefail
BIN="$HOME/.local/opt/rusty/current/rusty"
if [[ ! -x "$BIN" ]]; then
    message="Rusty is not installed. Run the project's scripts/install-rusty-local.sh"
    echo "$message" >&2
    if command -v zenity >/dev/null 2>&1; then
        zenity --error --title="Rusty" --text="$message" || true
    fi
    exit 1
fi
if [[ "${1:-}" = "--version" || "${1:-}" = "-V" ]]; then
    exec "$BIN" "$@"
fi
LOG_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/rusty"
mkdir -p "$LOG_DIR"
LOG="$LOG_DIR/launcher.log"
if [[ -f "$LOG" ]] && (( $(stat -c %s "$LOG") > 1048576 )); then
    mv -f "$LOG" "$LOG.old"
fi
printf '\n[%s] launching %s\n' "$(date -Is)" "$("$BIN" --version)" >> "$LOG"
exec "$BIN" "$@" >> "$LOG" 2>&1
LAUNCHER_EOF
chmod 0755 "$LAUNCHER_TMP"
mv -f -- "$LAUNCHER_TMP" "$LAUNCHER"
for legacy in slate-studio.desktop rustie-ux-review.desktop; do
  old="$HOME/.local/share/applications/$legacy"
  if [[ -f "$old" ]] && [[ ! -f "$HOME/.local/share/rusty/legacy-desktop/$legacy" ]]; then
    cp -p -- "$old" "$HOME/.local/share/rusty/legacy-desktop/$legacy"
  fi
done
APPS="$HOME/.local/share/applications"
cat > "$APPS/rusty.desktop" <<DESKTOP_EOF
[Desktop Entry]
Type=Application
Name=Rusty
GenericName=Digital Audio Workstation
Comment=$ACTUAL | Native Linux DAW
Exec=$HOME/.local/bin/rusty
TryExec=$HOME/.local/bin/rusty
Icon=multimedia-volume-control
Terminal=false
Categories=AudioVideo;Audio;Music;
StartupNotify=true
DESKTOP_EOF
# An old pinned shortcut still launches current Rusty, without a duplicate menu item.
cat > "$APPS/slate-studio.desktop" <<LEGACY_EOF
[Desktop Entry]
Type=Application
Name=Rusty (legacy shortcut)
GenericName=Digital Audio Workstation
Comment=Legacy shortcut, launches the current Rusty build
Exec=$HOME/.local/bin/rusty
TryExec=$HOME/.local/bin/rusty
Icon=multimedia-volume-control
Terminal=false
NoDisplay=true
Categories=AudioVideo;Audio;Music;
StartupNotify=true
LEGACY_EOF
REVIEW_PROJECT="$HOME/Music/Rustie UX Review/Rustie UX Review.slate.json"
if [[ -f "$REVIEW_PROJECT" ]]; then
  cat > "$APPS/rusty-ux-review.desktop" <<REVIEW_EOF
[Desktop Entry]
Type=Application
Name=Rusty (UX Review)
Comment=Opens the saved review song with the current Rusty build
Exec=$HOME/.local/bin/rusty --project "$REVIEW_PROJECT"
TryExec=$HOME/.local/bin/rusty
Icon=applications-multimedia
Terminal=false
Categories=AudioVideo;Audio;Music;
StartupNotify=true
REVIEW_EOF
  cat > "$APPS/rustie-ux-review.desktop" <<REVIEW_LEGACY_EOF
[Desktop Entry]
Type=Application
Name=Rusty (legacy UX Review shortcut)
Comment=Legacy shortcut, uses current Rusty binary
Exec=$HOME/.local/bin/rusty --project "$REVIEW_PROJECT"
TryExec=$HOME/.local/bin/rusty
Icon=applications-multimedia
Terminal=false
NoDisplay=true
Categories=AudioVideo;Audio;Music;
StartupNotify=true
REVIEW_LEGACY_EOF
fi
if command -v desktop-file-validate >/dev/null 2>&1; then
  desktop-file-validate "$APPS/rusty.desktop" "$APPS/slate-studio.desktop"
  if [[ -f "$APPS/rusty-ux-review.desktop" ]]; then
    desktop-file-validate "$APPS/rusty-ux-review.desktop" "$APPS/rustie-ux-review.desktop"
  fi
fi
# Atomic switch only after new executables and desktop-entry syntax are verified.
NEXT="$PREFIX/.current-next-$$"
ln -s "versions/$SHA" "$NEXT"
mv -Tf -- "$NEXT" "$PREFIX/current"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" || true
printf 'Installed: %s\nVersion:   %s\nApp menu:  Rusty\nLauncher:  %s\nWorker:    %s\n' \
  "$DEST" "$("$LAUNCHER" --version)" "$LAUNCHER" "$PREFIX/current/rusty-plugin-host"

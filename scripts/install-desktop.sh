#!/usr/bin/env bash
# Build the desktop app and install it as the `kelpie` command on this machine.
#
# Relaunches a running instance so a rebuild is actually visible — a stale
# installed binary is the failure this script exists to prevent. Pass
# --no-relaunch to leave the running app alone.
#
# Concurrent callers (parallel agent sessions, a commit hook racing a manual
# run) serialize on a lock so every caller installs its own source revision.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="${KELPIE_INSTALL_PATH:-$HOME/.local/bin/kelpie}"
built="$repo/desktop/src-tauri/target/release/kelpie-desktop"
relaunch=1

case "${1:-}" in
  --no-relaunch) relaunch=0 ;;
  --relaunch | "") ;;
  *)
    echo "usage: ${0##*/} [--relaunch|--no-relaunch]" >&2
    exit 2
    ;;
esac

umask 077
runtime_dir="${XDG_RUNTIME_DIR:-$HOME/.cache}/kelpie"
install -d -m 700 "$runtime_dir"
log() { printf '%s install-desktop: %s\n' "$(date +%H:%M:%S)" "$*"; }

exec 9>"$runtime_dir/install.lock"
flock 9

log "building release binary (bundling is off; binary only)"
cd "$repo/desktop"
env -u CI -u TAURI_CI npm run tauri -- build

mkdir -p "$(dirname "$target")"
# Install beside the target, then rename: atomic, and never writes through the
# inode of a running executable.
install -m 755 "$built" "$target.new"
mv -f "$target.new" "$target"
log "installed $target ($(md5sum "$target" | cut -c1-8))"

# Checked after the build, not before: an instance may start while we compile,
# and it would otherwise keep running the superseded image.
if ! pgrep -fx "$target" >/dev/null 2>&1; then
  log "app was not running; nothing to relaunch"
  exit 0
fi
if [ "$relaunch" = 0 ]; then
  log "relaunch disabled; restart kelpie to pick up this build"
  exit 0
fi
if [ -z "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ]; then
  log "no display in this environment; restart kelpie yourself to pick up this build"
  exit 0
fi

log "relaunching the running app"
pkill -fx "$target" || true
sleep 1
setsid nohup "$target" >"$runtime_dir/app.log" 2>&1 </dev/null 9>&- &
log "relaunched"

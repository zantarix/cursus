#!/usr/bin/env bash
# SessionStart hook — sets up the Nix dev shell for Claude Code web sessions.
#
# On first run: downloads and installs Nix + all dev-shell packages (~5 min).
# Subsequent sessions reuse the cached container state and are near-instant.
set -euo pipefail

# Only activate inside Claude Code remote (web) sessions.
if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
    exit 0
fi

cd "$CLAUDE_PROJECT_DIR"

# ── 1. Install Nix if not present ─────────────────────────────────────────────
if ! command -v nix &>/dev/null; then
    echo "[session-start] Installing Nix (Determinate Systems)..."
    curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix \
        | sh -s -- install linux \
            --no-confirm \
            --init none \
            --extra-conf "sandbox = false"
fi

# Source the Nix profile so `nix` is on PATH for the rest of this script.
NIX_DAEMON_PROFILE=/nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
if [ -f "$NIX_DAEMON_PROFILE" ]; then
    # shellcheck source=/dev/null
    . "$NIX_DAEMON_PROFILE"
fi

# ── 2. Ensure the Nix daemon is running ───────────────────────────────────────
# The daemon is required by the multi-user Nix install (what Determinate uses).
if ! pgrep -x nix-daemon >/dev/null 2>&1; then
    echo "[session-start] Starting nix-daemon..."
    /nix/var/nix/profiles/default/bin/nix-daemon &
    # Give it a moment to create its socket.
    sleep 3
fi

# ── 3. Build the dev shell and export its environment ─────────────────────────
echo "[session-start] Materialising Nix dev shell (first run downloads packages)..."

# Run a single command inside `nix develop` to capture both PATH and RUST_SRC_PATH
# --no-update-lock-file prevents accidental writes to flake.lock
eval "$(
    nix develop --no-update-lock-file --command bash -c \
        'printf "DEV_PATH=%s\nDEV_RUST_SRC=%s\n" "$PATH" "${RUST_SRC_PATH:-}"'
)"

# Persist the dev-shell environment for all tools Claude will invoke this session.
{
    echo "export PATH=\"${DEV_PATH}\""
    if [ -n "${DEV_RUST_SRC:-}" ]; then
        echo "export RUST_SRC_PATH=\"${DEV_RUST_SRC}\""
    fi
} >> "$CLAUDE_ENV_FILE"

echo "[session-start] Done. $(nix develop --no-update-lock-file --command rustc --version)"

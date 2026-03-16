#!/bin/bash
set -e

# Automatically enter the flake's dev shell for new interactive sessions.
# CURSUS_DEV_SHELL is exported before exec so it survives through the nix
# develop invocation into the spawned shell, preventing re-entry on that
# shell's .bashrc. IN_NIX_SHELL is not reliable here as nix sets it after
# the shell starts rather than before .bashrc is sourced.
cat >> ~/.bashrc << 'EOF'
if [[ -z "$CURSUS_DEV_SHELL" && $- == *i* ]]; then
    export CURSUS_DEV_SHELL=1
    exec nix develop
fi
EOF

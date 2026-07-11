#!/usr/bin/env bash
# Update Ludex in place (run INSIDE the container, or via `pct exec`).
# Re-fetches the app + rebuilds; your data, library config and password
# (in /opt/ludex-data) are preserved.
set -euo pipefail
export LANG=C.UTF-8 LC_ALL=C.UTF-8

RAW="https://raw.githubusercontent.com/rajat10cube/ludex/main"
REPO="https://github.com/rajat10cube/ludex"

curl -fsSL "$RAW/deploy/lxc/ludex-install.sh" -o /tmp/ludex-install.sh
LUDEX_REPO="$REPO" bash /tmp/ludex-install.sh
rm -f /tmp/ludex-install.sh
echo "[ludex] update complete."

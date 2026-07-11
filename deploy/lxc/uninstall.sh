#!/usr/bin/env bash
# Remove Ludex (run INSIDE the container, or via `pct exec`).
# Keeps your data by default; pass --purge to also delete it.
#
#   bash uninstall.sh            # remove app, keep /opt/ludex-data
#   bash uninstall.sh --purge    # also delete data + service user
set -euo pipefail

systemctl disable --now ludex 2>/dev/null || true
rm -f /etc/systemd/system/ludex.service
systemctl daemon-reload 2>/dev/null || true
rm -rf /opt/ludex

if [ "${1:-}" = "--purge" ]; then
  rm -rf /opt/ludex-data
  id ludex >/dev/null 2>&1 && deluser ludex 2>/dev/null || true
  echo "[ludex] fully removed (including data)."
else
  echo "[ludex] removed. Data kept at /opt/ludex-data (re-run with --purge to delete)."
fi
echo "[ludex] To remove the whole container, on the host run: pct stop <CTID> && pct destroy <CTID>"

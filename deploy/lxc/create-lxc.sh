#!/usr/bin/env bash
# Create a Debian 12 LXC on a Proxmox host and install Ludex into it.
# Run this ON THE PROXMOX HOST (as root).
#
# One-liner (fetches everything from GitHub):
#   MEDIA_HOST=/mnt/pool/games CTID=120 \
#     bash -c "$(curl -fsSL https://raw.githubusercontent.com/rajat10cube/ludex/main/deploy/lxc/create-lxc.sh)"
#
# From a local clone instead (copy local source into the CT):
#   MEDIA_HOST=/mnt/pool/games CTID=120 LUDEX_REPO= bash deploy/lxc/create-lxc.sh
set -euo pipefail

CTID="${CTID:?Set CTID=<unused container id>, e.g. CTID=120}"
CT_HOSTNAME="${CT_HOSTNAME:-ludex}"        # note: not HOSTNAME (shell builtin = the PVE host's name)
CORES="${CORES:-2}"
RAM_MB="${RAM_MB:-2048}"
DISK_GB="${DISK_GB:-10}"
BRIDGE="${BRIDGE:-vmbr0}"
STORAGE="${STORAGE:-local-lvm}"            # rootfs storage
TEMPLATE_STORAGE="${TEMPLATE_STORAGE:-local}"
UNPRIVILEGED="${UNPRIVILEGED:-1}"          # 1=unprivileged (safer). See media note.
MEDIA_HOST="${MEDIA_HOST:-}"               # host path to your games (bind-mounted RO)
MEDIA_CT="${MEDIA_CT:-/libraries/games}"
LUDEX_AUTH_PASS="${LUDEX_AUTH_PASS:-}"     # empty -> create the admin on first login (UI)

# Where to get Ludex from. Default = clone the public repo (works for curl|bash).
# Set LUDEX_REPO= (empty) when running from a local clone to copy local source.
LUDEX_REPO="${LUDEX_REPO-https://github.com/rajat10cube/ludex}"
LUDEX_RAW="${LUDEX_RAW:-https://raw.githubusercontent.com/rajat10cube/ludex/main}"

msg() { echo -e "\e[1;34m[ludex]\e[0m $*"; }
die() { echo -e "\e[1;31m[ludex] $*\e[0m" >&2; exit 1; }
command -v pct >/dev/null || die "pct not found - run this on the Proxmox host."
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || echo "")"

# --- ensure a debian-12 template is available ---
msg "Locating Debian 12 template..."
TPL=$(pveam list "$TEMPLATE_STORAGE" 2>/dev/null | awk '{print $1}' | grep -m1 'debian-12-standard' || true)
if [ -z "$TPL" ]; then
  AVAIL=$(pveam available --section system | awk '{print $2}' | grep -m1 'debian-12-standard') \
    || die "no debian-12-standard template available via pveam"
  msg "Downloading $AVAIL..."
  pveam download "$TEMPLATE_STORAGE" "$AVAIL"
  TPL="$TEMPLATE_STORAGE:vztmpl/$AVAIL"
fi
msg "Template: $TPL"

# --- create + start container ---
msg "Creating CT $CTID ($CT_HOSTNAME: ${CORES}c/${RAM_MB}MB/${DISK_GB}GB)..."
pct create "$CTID" "$TPL" \
  -hostname "$CT_HOSTNAME" \
  -cores "$CORES" -memory "$RAM_MB" -swap 512 \
  -rootfs "$STORAGE:${DISK_GB}" \
  -net0 "name=eth0,bridge=$BRIDGE,ip=dhcp" \
  -features nesting=1 \
  -unprivileged "$UNPRIVILEGED" \
  -onboot 1

if [ -n "$MEDIA_HOST" ]; then
  msg "Bind-mounting $MEDIA_HOST -> $MEDIA_CT (read-only)..."
  pct set "$CTID" -mp0 "$MEDIA_HOST,mp=$MEDIA_CT,ro=1"
fi

pct start "$CTID"
msg "Waiting for network..."
for _ in $(seq 1 30); do pct exec "$CTID" -- test -e /etc/resolv.conf && break; sleep 1; done
pct exec "$CTID" -- bash -c 'for i in $(seq 1 30); do getent hosts deb.debian.org >/dev/null && break; sleep 1; done'

# --- deliver source (only when NOT cloning from git) ---
pct exec "$CTID" -- mkdir -p /opt/ludex
if [ -z "$LUDEX_REPO" ]; then
  [ -n "$SCRIPT_DIR" ] || die "local-source mode needs to run from a clone (set LUDEX_REPO=URL instead)"
  REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
  msg "Copying local source ($REPO_ROOT) into the container..."
  TARBALL="/tmp/ludex-src-$CTID.tar.gz"
  tar -czf "$TARBALL" -C "$REPO_ROOT" \
    --exclude='.git' --exclude='node_modules' --exclude='.venv' \
    --exclude='**/data' --exclude='backend/.env' --exclude='backend/data' .
  pct push "$CTID" "$TARBALL" /tmp/ludex-src.tar.gz
  pct exec "$CTID" -- tar -xzf /tmp/ludex-src.tar.gz -C /opt/ludex
  rm -f "$TARBALL"
fi

# --- get the in-container installer (local file or fetch from GitHub raw) ---
INSTALLER="$SCRIPT_DIR/ludex-install.sh"
if [ ! -f "$INSTALLER" ]; then
  INSTALLER="/tmp/ludex-install.sh"
  msg "Fetching installer from $LUDEX_RAW..."
  curl -fsSL "$LUDEX_RAW/deploy/lxc/ludex-install.sh" -o "$INSTALLER" \
    || die "could not download ludex-install.sh"
fi
pct push "$CTID" "$INSTALLER" /root/ludex-install.sh -perms 755

# --- run installer inside the container ---
msg "Running installer..."
pct exec "$CTID" -- env \
  LUDEX_REPO="$LUDEX_REPO" \
  LUDEX_AUTH_PASS="$LUDEX_AUTH_PASS" \
  bash /root/ludex-install.sh

IP=$(pct exec "$CTID" -- hostname -I 2>/dev/null | awk '{print $1}')
msg "All set  Ludex -> http://${IP:-<ct-ip>}:8000"
if [ -n "$LUDEX_AUTH_PASS" ]; then
  msg "Login: admin / $LUDEX_AUTH_PASS"
else
  msg "Create your admin (master) account on first login."
fi
[ "$UNPRIVILEGED" = "1" ] && [ -n "$MEDIA_HOST" ] && {
  msg "NOTE (unprivileged): if games don't appear, files must be readable by the"
  msg "      mapped UID - 'chmod -R o+rX $MEDIA_HOST' on the host, or recreate with UNPRIVILEGED=0."
} || true

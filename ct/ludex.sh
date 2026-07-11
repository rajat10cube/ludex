#!/usr/bin/env bash
# Ludex - Proxmox VE LXC installer (community-scripts style, interactive).
#
# Run on the Proxmox host:
#   bash -c "$(curl -fsSL https://raw.githubusercontent.com/rajat10cube/ludex/main/ct/ludex.sh)"
#
# Pick Default (presets) or Advanced (set CT id, resources, storage, etc.).
# You create the admin account on first login in the web UI. Every value can be
# preset via env (CTID, CT_HOSTNAME, CORES, RAM_MB, DISK_GB, STORAGE, BRIDGE,
# UNPRIVILEGED) to run non-interactively.
set -euo pipefail

REPO="https://github.com/rajat10cube/ludex"
RAW="https://raw.githubusercontent.com/rajat10cube/ludex/main"

YW="\e[33m"; GN="\e[1;92m"; RD="\e[31m"; BL="\e[36m"; CL="\e[0m"
msg() { echo -e "${YW}[ludex]${CL} $*"; }
ok()  { echo -e "${GN}[ludex]${CL} $*"; }
die() { echo -e "${RD}[ludex] $*${CL}" >&2; exit 1; }

header() {
  echo -e "${BL}"
  cat <<'EOF'
  _    _   _ ___  _____  __
 | |  | | | |   \| __\ \/ /
 | |__| |_| | |) | _| >  <
 |____|\___/|___/|___/_/\_\
      self-hosted game library
EOF
  echo -e "${CL}"
}

command -v pct >/dev/null || die "Run this on a Proxmox VE host (pct not found)."
[ "$(id -u)" -eq 0 ] || die "Run as root."

pick_storage() { # $1 = content type (rootdir | vztmpl)
  local list
  list=$(pvesm status --content "$1" 2>/dev/null | awk 'NR>1 {print $1}')
  echo "$list" | grep -qx "local-lvm" && { echo "local-lvm"; return; }
  echo "$list" | head -1
}

# --- defaults (all env-overridable) ---
CT_HOSTNAME="${CT_HOSTNAME:-ludex}"
CORES="${CORES:-2}"
RAM_MB="${RAM_MB:-2048}"
DISK_GB="${DISK_GB:-10}"
BRIDGE="${BRIDGE:-vmbr0}"
UNPRIVILEGED="${UNPRIVILEGED:-1}"
STORAGE="${STORAGE:-$(pick_storage rootdir)}"
TEMPLATE_STORAGE="${TEMPLATE_STORAGE:-$(pick_storage vztmpl)}"
CTID="${CTID:-$(pvesh get /cluster/nextid 2>/dev/null || true)}"
[ -n "${TEMPLATE_STORAGE:-}" ] || TEMPLATE_STORAGE="local"
[ -n "${STORAGE:-}" ] || die "no storage supporting container rootfs found (set STORAGE=)."
[ -n "${CTID:-}" ] || die "could not determine a CT ID (set CTID=)."

header

# --- Default / Advanced (whiptail when we have a TTY) ---
if [ -t 0 ] && command -v whiptail >/dev/null; then
  MODE=$(whiptail --title "Ludex LXC" --menu \
"How do you want to install Ludex?" 13 64 2 \
    "default"  "Presets (quick)" \
    "advanced" "Set CT id, resources, storage..." \
    3>&1 1>&2 2>&3) || die "cancelled"

  if [ "$MODE" = "advanced" ]; then
    CTID=$(whiptail --title "Advanced" --inputbox "Container ID" 8 60 "$CTID" 3>&1 1>&2 2>&3) || die "cancelled"
    CT_HOSTNAME=$(whiptail --title "Advanced" --inputbox "Hostname" 8 60 "$CT_HOSTNAME" 3>&1 1>&2 2>&3) || die "cancelled"
    CORES=$(whiptail --title "Advanced" --inputbox "CPU cores" 8 60 "$CORES" 3>&1 1>&2 2>&3) || die "cancelled"
    RAM_MB=$(whiptail --title "Advanced" --inputbox "RAM (MB)" 8 60 "$RAM_MB" 3>&1 1>&2 2>&3) || die "cancelled"
    DISK_GB=$(whiptail --title "Advanced" --inputbox "Disk (GB)" 8 60 "$DISK_GB" 3>&1 1>&2 2>&3) || die "cancelled"
    STORAGE=$(whiptail --title "Advanced" --inputbox "Storage (rootfs)" 8 60 "$STORAGE" 3>&1 1>&2 2>&3) || die "cancelled"
    BRIDGE=$(whiptail --title "Advanced" --inputbox "Network bridge" 8 60 "$BRIDGE" 3>&1 1>&2 2>&3) || die "cancelled"
    if whiptail --title "Privilege" --yesno "Unprivileged container? (recommended)\n\nChoose <No> only if your game files are not world-readable." 12 64; then
      UNPRIVILEGED=1; else UNPRIVILEGED=0; fi
  fi
  whiptail --title "Confirm" --yesno \
"Create CT $CTID ($CT_HOSTNAME)?\n
  Cores: $CORES   RAM: ${RAM_MB}MB   Disk: ${DISK_GB}GB
  Storage: $STORAGE   Bridge: $BRIDGE
  Unprivileged: $([ "$UNPRIVILEGED" = 1 ] && echo yes || echo no)

You'll add your game folders afterwards in the web UI." 16 64 || die "cancelled"
else
  msg "Non-interactive - using defaults/env (CTID=$CTID, storage=$STORAGE)."
fi

# --- ensure a Debian 12 template ---
msg "Locating Debian 12 template..."
TPL=$(pveam list "$TEMPLATE_STORAGE" 2>/dev/null | awk '{print $1}' | grep -m1 'debian-12-standard' || true)
if [ -z "$TPL" ]; then
  AVAIL=$(pveam available --section system | awk '{print $2}' | grep -m1 'debian-12-standard') \
    || die "no debian-12-standard template available via pveam."
  msg "Downloading $AVAIL..."
  pveam download "$TEMPLATE_STORAGE" "$AVAIL" >/dev/null
  TPL="$TEMPLATE_STORAGE:vztmpl/$AVAIL"
fi

# --- create + start ---
msg "Creating CT $CTID ($CT_HOSTNAME)..."
pct create "$CTID" "$TPL" \
  -hostname "$CT_HOSTNAME" -cores "$CORES" -memory "$RAM_MB" -swap 512 \
  -rootfs "$STORAGE:${DISK_GB}" -net0 "name=eth0,bridge=$BRIDGE,ip=dhcp" \
  -features nesting=1 -unprivileged "$UNPRIVILEGED" -onboot 1 >/dev/null

msg "Starting container..."
pct start "$CTID"
for _ in $(seq 1 30); do pct exec "$CTID" -- test -e /etc/resolv.conf 2>/dev/null && break; sleep 1; done
pct exec "$CTID" -- bash -c 'for i in $(seq 1 30); do getent hosts deb.debian.org >/dev/null 2>&1 && break; sleep 1; done'

# --- fetch installer on the host, push it in, run it ---
msg "Fetching installer..."
curl -fsSL "$RAW/deploy/lxc/ludex-install.sh" -o /tmp/ludex-install.sh || die "could not download installer"
pct push "$CTID" /tmp/ludex-install.sh /root/ludex-install.sh -perms 755
rm -f /tmp/ludex-install.sh

msg "Installing Ludex (clones repo + builds frontend; a few minutes)..."
pct exec "$CTID" -- env \
  LUDEX_REPO="$REPO" \
  bash /root/ludex-install.sh

IP=$(pct exec "$CTID" -- hostname -I 2>/dev/null | awk '{print $1}')
echo
ok "Done!  Ludex -> http://${IP:-<ct-ip>}:8000"
ok "Open it and create your admin (master) account on first login."
echo
msg "Then add your games in the web UI -> Libraries. First make them visible to the CT, e.g.:"
msg "  pct set $CTID -mp0 /your/host/games,mp=/mnt/games,ro=1 && pct reboot $CTID"
msg "then add the path /mnt/games in the UI."
[ "$UNPRIVILEGED" = "1" ] && msg "(unprivileged: if the folder isn't readable inside the CT, run 'chmod -R o+rX' on it on the host.)"

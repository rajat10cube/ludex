# Ludex on Proxmox (LXC)

A native LXC install (no Docker) with a `systemd` service - the same convenient
shape as the [community-scripts.org](https://community-scripts.org/) helpers, but
runnable on *your* host today.

## Quick start (on the Proxmox host, as root)

**Interactive one-liner** (recommended) - auto-picks the next CT ID and prompts
for resources (Default/Advanced). You add your game folders afterwards in the web UI:
```bash
bash -c "$(curl -fsSL https://raw.githubusercontent.com/rajat10cube/ludex/main/ct/ludex.sh)"
```

**Non-interactive / scriptable** - drive `create-lxc.sh` with env vars:
```bash
MEDIA_HOST=/mnt/pool/games CTID=120 LUDEX_AUTH_PASS='supersecret' \
  bash -c "$(curl -fsSL https://raw.githubusercontent.com/rajat10cube/ludex/main/deploy/lxc/create-lxc.sh)"
```

From a local clone (copies your local source instead of cloning the repo):
```bash
git clone https://github.com/rajat10cube/ludex && cd ludex
MEDIA_HOST=/mnt/pool/games CTID=120 LUDEX_REPO= bash deploy/lxc/create-lxc.sh
```

That creates a Debian 12 LXC, builds + installs Ludex, and starts it on
`http://<container-ip>:8000`. You then add game folders in the app (below).

### Knobs (env vars for `create-lxc.sh`)
| Var | Default | Meaning |
|-----|---------|---------|
| `CTID` | *(required)* | unused container id, e.g. `120` |
| `MEDIA_HOST` | - | optional: host path to pre-mount read-only into the CT |
| `MEDIA_CT` | `/libraries/games` | mount point inside the CT |
| `CT_HOSTNAME` | `ludex` | container hostname |
| `CORES` / `RAM_MB` / `DISK_GB` | `2` / `2048` / `10` | resources |
| `BRIDGE` / `STORAGE` | `vmbr0` / `local-lvm` | network / rootfs storage |
| `UNPRIVILEGED` | `1` | `0` = privileged (simplest for media perms) |
| `LUDEX_REPO` | (repo) | set empty to copy local source instead of cloning |
| `LUDEX_AUTH_PASS` | - | Basic-auth password (empty = create admin in UI) |

## Adding your games (in the app, like Jellyfin)
Games are added from the **web UI -> Libraries**. Two steps:

1. **Make the folder visible to the container** - bind-mount your host games
   into the CT (read-only), then reboot it:
   ```bash
   pct set <CTID> -mp0 /mnt/pool/games,mp=/mnt/games,ro=1
   pct reboot <CTID>
   ```
   Add more sources with `-mp1`, `-mp2`, ...
2. **Add it in Ludex** - open the app -> **Libraries** -> type or Browse to
   `/mnt/games` -> it scans automatically. Repeat for each folder.

**Unprivileged note:** if a mounted folder isn't readable inside the CT, on the
host run `chmod -R o+rX /mnt/pool/games` (or recreate with `UNPRIVILEGED=0`).

> The library holds the *master copies*. Actual gameplay happens on your Windows
> laptop via the companion agent, which downloads each game from this server on
> demand. The CT only needs enough disk for its own OS, not for the games.

## Day-2

```bash
pct exec 120 -- systemctl status ludex
pct exec 120 -- journalctl -u ludex -f          # logs
# rescan after adding games:
curl -u admin:PASS -X POST http://<ip>:8000/api/libraries/scan
```

Put it behind your reverse proxy as usual - see [../../docs/DEPLOY.md](../../docs/DEPLOY.md).

## Update
Re-fetches the app and rebuilds; data/config/password are preserved.
```bash
pct exec <CTID> -- bash -c "$(curl -fsSL https://raw.githubusercontent.com/rajat10cube/ludex/main/deploy/lxc/update.sh)"
```

## Uninstall
```bash
# remove the app, keep your data:
pct exec <CTID> -- bash -c "$(curl -fsSL https://raw.githubusercontent.com/rajat10cube/ludex/main/deploy/lxc/uninstall.sh)"
# also delete data + service user:
pct exec <CTID> -- bash -c "$(curl -fsSL https://raw.githubusercontent.com/rajat10cube/ludex/main/deploy/lxc/uninstall.sh)" ludex --purge
# or remove the whole container from the host:
pct stop <CTID> && pct destroy <CTID>
```

## Manual install (existing container)
Already have a Debian/Ubuntu LXC? Just run the in-container installer:
```bash
LUDEX_REPO=https://github.com/rajat10cube/ludex bash ludex-install.sh
```

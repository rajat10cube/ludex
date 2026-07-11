# Ludex

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)

A self-hosted **"Steam for your own game folders."** Point it at folders of PC
games on your home server and get a browsable, searchable library. Click
**Install** on your Windows laptop and a companion agent downloads the game from
the server, extracts/installs it, launches it, and tracks your playtime - all
from any browser.

> Built for the homelab/Proxmox use case: the server *holds and serves* games you
> already have organized in folders; a small Windows agent *installs and plays*
> them on your PC. The server never runs the games itself.

## How it fits together

```
   Proxmox server (Ludex)                Windows laptop
   +----------------------+              +-----------------------+
   |  scans game folders  |   HTTP/tar   |  companion agent      |
   |  library + web UI    |  <--------->  |  ludex:// handler     |
   |  download + playtime |   (Basic)    |  download/extract/run |
   +----------------------+              +-----------------------+
```

## Torrent-repack aware

Games come in every shape a PC torrent does, and Ludex classifies each one so the
agent knows what to do:

- **Portable folder** - extract and run the `.exe` directly.
- **Installer** - a loose `setup.exe` / `.msi`; the agent runs it.
- **Disc image** - an `.iso` (often inside a folder with `.nfo`/readme); the agent
  mounts it and runs the setup.
- **Archive** - a loose `.zip` (auto-extracted) or `.7z`/`.rar`.
- **Denuvo / hypervisor (DenuvOwO)** - flagged with a **DRM** badge. Ludex surfaces
  the release's own `VBS.cmd` / "HOW TO USE" steps but **never disables Windows
  security for you** - turning off Virtualization-Based Security + Driver Signature
  Enforcement (and the reboot + F7 at boot) stays a manual, informed choice.

The scanner also cleans scene names (`Hades.v1.38299.[FitGirl.Repack]` ->
**Hades**, version `1.38299`), detects the release group, collapses redundant
nesting (`Crimson.Desert/Crimson.Desert/...`), finds the main executable, and
captures cover art + release notes.

## Features
- **Scanner** - top-level folder / loose file -> Game, with setup-method
  classification, DRM detection, title/version cleanup, cover + exe detection,
  size totals, missing-file marking, and a rescan endpoint.
- **Library UI** - dark, Steam-like grid with search, sort, and Installed/DRM
  filters; a detail panel with Install / Play / Download, install-state badges,
  playtime, and per-game release notes.
- **Windows agent** - a PowerShell companion that registers the `ludex://`
  protocol, downloads + extracts games, launches them (reporting playtime), and
  handles installers/ISOs. One-line install from the app.
- **Accounts** - **first-run admin signup**, then cookie-session login with
  multiple users; each user gets their own installs + playtime. Hashed passwords
  (PBKDF2). HTTP Basic also works (used by the agent).
- **Libraries** - add/remove game folders from the web UI (like Jellyfin), with a
  built-in folder browser; auto-scans on add.
- **Deploy** - multi-stage Docker + healthcheck, SPA deep-link fallback,
  reverse-proxy / subpath support, and Proxmox LXC install scripts.

## Stack
Python 3.12 · FastAPI · SQLite (-> Postgres later) · React + TypeScript · Docker.
The Windows agent is dependency-free PowerShell (uses the built-in `tar.exe`).

## Repo layout
```
backend/    FastAPI app, SQLAlchemy models, scanner, tests
frontend/   React + TS SPA (Vite) - builds into backend/app/static
client/     Windows companion agent (install.ps1 + ludex-agent.ps1)
deploy/     Proxmox LXC install scripts
docs/       deployment guide
Dockerfile, docker-compose.yml, ludex.yaml.example
```

## Dev quickstart

**Backend** (from `backend/`):
```bash
python -m venv .venv
. .venv/Scripts/activate        # Windows;  use .venv/bin/activate on Linux/macOS
pip install -r requirements-dev.txt
cp .env.example .env            # set LUDEX_GAMES_DIR or LUDEX_CONFIG
uvicorn app.main:app --reload   # http://localhost:8000  (docs at /docs)
pytest                          # smoke tests
```

**Frontend** (from `frontend/`):
```bash
npm install
npm run dev                     # http://localhost:5173 (proxies /api -> :8000)
```

## Deploy

**Docker:**
```bash
cp ludex.yaml.example ludex.yaml           # optional; or add libraries in the UI
# edit docker-compose.yml volume paths + LUDEX_AUTH_PASS
docker compose up --build                  # http://<host>:8810
```

**Proxmox LXC (no Docker)** - run on the PVE host; it's interactive
(Default/Advanced, auto-picks the CT ID) and installs Ludex as a `systemd`
service. You add your game folders afterwards in the app:
```bash
bash -c "$(curl -fsSL https://raw.githubusercontent.com/rajat10cube/ludex/main/ct/ludex.sh)"
```
See **[deploy/lxc/README.md](deploy/lxc/README.md)**. Reverse proxy:
**[docs/DEPLOY.md](docs/DEPLOY.md)**. Mount game libraries read-only; app state
lives in a data volume. Put it behind your existing reverse proxy / VPN.

## The Windows agent
In the app, open **Settings -> Windows companion agent** and run the one-liner in
an admin PowerShell window:
```powershell
irm http://<host>:8810/api/client/install.ps1 | iex
```
It registers the `ludex://` handler, signs in with your Ludex account (stored
DPAPI-encrypted), and picks an install folder. Then **Install** / **Play** on any
game just work. Re-run the one-liner any time to update the agent.

## Accounts
On first launch Ludex shows a **one-time signup** to create your **master admin**
account. After that it's a normal **login page** (cookie session). For automation
you can pre-create the admin with `LUDEX_AUTH_USER` / `LUDEX_AUTH_PASS` (skips
signup). Set `LUDEX_AUTH=none` to disable auth entirely (single-user, LAN/VPN only).

## License
Licensed under the **GNU Affero General Public License v3.0** - see [LICENSE](LICENSE).
AGPL covers network use: if you run a modified Ludex as a network service, you
must offer its users the corresponding modified source.

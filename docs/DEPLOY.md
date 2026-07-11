# Deploying Ludex (Proxmox / homelab)

Ludex ships as one Docker image: the React SPA is built and served by the
FastAPI app, which also indexes your game folders and streams downloads to the
Windows companion agent.

> **Where games run:** Ludex stores the *library* (master copies) on your server
> and serves them on demand. Games are downloaded, installed, and played on your
> **Windows laptop** by the companion agent (`Settings -> Windows companion
> agent`). The server never runs the games itself.

## 1. docker compose

```bash
cp ludex.yaml.example ludex.yaml          # optional; or add libraries in the UI
# edit docker-compose.yml volume paths + LUDEX_AUTH_PASS
docker compose up --build                 # http://<host>:8810
```

- Mount your game libraries **read-only** (`:ro`).
- App state (SQLite + secret key) lives in the named volume `ludex-data`.
- Defaults to HTTP Basic auth `admin` / `change-me` - **change `LUDEX_AUTH_PASS`.**
- The container has a `HEALTHCHECK` hitting `/api/health`.

Trigger a rescan after adding games:
```bash
curl -u admin:PASS -X POST http://<host>:8810/api/libraries/scan
```

## 2. Install the Windows agent

On your laptop, open the app -> **Settings -> Windows companion agent** and copy
the one-liner into an **admin PowerShell** window:
```powershell
irm http://<host>:8810/api/client/install.ps1 | iex
```
It registers the `ludex://` protocol, stores your login (DPAPI-encrypted), and
asks where to install games. After that, **Install** / **Play** buttons work.

## 3. Behind your reverse proxy

The app serves everything from the **root** path and uses absolute URLs for its
assets/API. Prefer a **subdomain**, or a subpath proxy that **strips the prefix**.

### Caddy (recommended)
```caddy
games.example.com {
    reverse_proxy 127.0.0.1:8810
}
```

### Nginx Proxy Manager
- New Proxy Host -> Forward to `ludex:8000` (or `host:8810`).
- Increase proxy read timeout (large downloads): set `proxy_read_timeout 3600;`
  and `proxy_buffering off;` in the Advanced tab. **Disable any body-size cap**
  (`client_max_body_size 0;`) - game downloads are tens to hundreds of GB.

### Traefik (labels on the compose service)
```yaml
labels:
  - traefik.enable=true
  - traefik.http.routers.ludex.rule=Host(`games.example.com`)
  - traefik.http.services.ludex.loadbalancer.server.port=8000
```

### Subpath (e.g. https://home.example.com/games)
Strip the prefix at the proxy, and tell the app its public base for correct
`/docs` + OpenAPI links:
```caddy
home.example.com {
    handle_path /games/* {
        reverse_proxy 127.0.0.1:8810
    }
}
```
```yaml
# docker-compose.yml -> environment
LUDEX_BASE_PATH: /games     # used as FastAPI root_path
```
> If your proxy does **not** strip the prefix, host Ludex on its own subdomain
> instead - the SPA's absolute asset paths assume root.

## 4. Notes

- **Large downloads:** folder games are streamed as an uncompressed `.tar` with
  chunked transfer (no length header). Make sure the proxy allows long-lived
  responses and doesn't buffer the whole body. Loose files support HTTP `Range`
  (`206 Partial Content`) so those downloads resume - don't strip `Range`.
- **DRM / hypervisor games:** Ludex flags Denuvo/hypervisor releases and shows
  the release's own instructions, but it never disables Windows security for you.
  Those steps (VBS/DSE off + reboot) are always manual on the laptop.
- **VPN-only:** set `LUDEX_AUTH=none` and rely on Tailscale/WireGuard/LAN. The
  agent still needs a reachable server URL.

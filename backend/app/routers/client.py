"""Serve the Windows companion-agent scripts, with the server URL injected.

These scripts contain no secrets, so they're unauthenticated — the bootstrap
one-liner (``irm <server>/api/client/install.ps1 | iex``) works out of the box on
your LAN/VPN. The user still logs in with their Ludex account when the agent runs.
"""

from __future__ import annotations

from fastapi import APIRouter, HTTPException, Request
from fastapi.responses import PlainTextResponse

from ..config import get_settings

router = APIRouter(prefix="/client", tags=["client"])

_SCRIPTS = {
    "install.ps1": "install.ps1",
    "ludex-agent.ps1": "ludex-agent.ps1",
}


def _server_url(request: Request) -> str:
    """Best-effort external base URL (honours reverse-proxy X-Forwarded-* headers)."""
    base = str(request.base_url).rstrip("/")
    root = get_settings().base_path.rstrip("/")
    if root and root not in ("", "/") and not base.endswith(root):
        base = f"{base}{root}"
    return base


def _render(name: str, request: Request) -> str:
    path = get_settings().client_scripts_dir() / _SCRIPTS[name]
    if not path.is_file():
        raise HTTPException(404, f"{name} is not bundled with this server")
    text = path.read_text(encoding="utf-8")
    return text.replace("@@SERVER_URL@@", _server_url(request))


@router.get("/{name}", response_class=PlainTextResponse)
def get_script(name: str, request: Request) -> PlainTextResponse:
    if name not in _SCRIPTS:
        raise HTTPException(404, "Unknown script")
    return PlainTextResponse(_render(name, request), media_type="text/plain; charset=utf-8")

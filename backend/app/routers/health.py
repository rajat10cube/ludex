"""Liveness endpoint (unauthenticated; used by Docker healthcheck)."""

from __future__ import annotations

from fastapi import APIRouter

from .. import __version__

router = APIRouter(tags=["health"])


@router.get("/health")
def health() -> dict:
    return {"status": "ok", "service": "ludex", "version": __version__}

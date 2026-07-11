"""UI-managed settings — currently the optional artwork provider keys.

Reads never echo a stored secret back; they return only whether each key is set,
where it came from (saved vs env), and a short masked hint.
"""

from __future__ import annotations

from fastapi import APIRouter, Depends
from pydantic import BaseModel

from .. import settings_store
from ..artwork import ArtworkService
from ..auth import require_admin
from ..config import get_settings

router = APIRouter(prefix="/settings", tags=["settings"], dependencies=[Depends(require_admin)])


class ArtworkKeysIn(BaseModel):
    # Only non-empty fields are applied; omit a field to leave it unchanged.
    steamgriddb_key: str | None = None
    igdb_client_id: str | None = None
    igdb_client_secret: str | None = None
    clear: list[str] | None = None  # keys to explicitly clear (fall back to env)


def _field_state(key: str, env_value: str) -> dict:
    saved = settings_store.get(key)
    effective = saved or (env_value or None)
    source = "saved" if saved else ("env" if env_value else None)
    hint = None
    if effective:
        hint = ("*" * 4) + effective[-4:] if len(effective) >= 4 else "set"
    return {"set": bool(effective), "source": source, "hint": hint}


@router.get("/artwork")
def get_artwork_settings() -> dict:
    s = get_settings()
    art = ArtworkService()
    return {
        "steamgriddb_key": _field_state("steamgriddb_key", s.steamgriddb_key),
        "igdb_client_id": _field_state("igdb_client_id", s.igdb_client_id),
        "igdb_client_secret": _field_state("igdb_client_secret", s.igdb_client_secret),
        "steamgriddb": art.sgdb is not None,
        "igdb": art.igdb is not None,
        "enabled": art.enabled(),
    }


@router.post("/artwork")
def save_artwork_settings(body: ArtworkKeysIn) -> dict:
    updates: dict[str, str | None] = {}
    for key in ("steamgriddb_key", "igdb_client_id", "igdb_client_secret"):
        value = getattr(body, key)
        if value:  # only apply typed values
            updates[key] = value.strip()
    for key in body.clear or []:
        if key in settings_store.ALLOWED_KEYS:
            updates[key] = None
    if updates:
        settings_store.set_many(updates)
    return get_artwork_settings()

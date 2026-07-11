"""Endpoints used by the Windows companion agent.

The agent authenticates with HTTP Basic (the same account as the web UI) and
uses these to register the machine, report which games are installed, and log
play sessions (which drive playtime + last-played in the library).
"""

from __future__ import annotations

from datetime import UTC, datetime

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel
from sqlalchemy import delete, select
from sqlalchemy.orm import Session

from .. import __version__
from ..auth import require_user
from ..db import get_db
from ..models import Device, Game, Installation, PlaySession, User
from .games import DOWNLOAD_KIND, download_name

router = APIRouter(prefix="/agent", tags=["agent"], dependencies=[Depends(require_user)])


class HelloIn(BaseModel):
    device: str
    platform: str = "windows"
    agent_version: str | None = None


class InstalledGame(BaseModel):
    slug: str
    version: str | None = None
    install_path: str | None = None


class InstalledIn(BaseModel):
    device: str
    games: list[InstalledGame]


class SessionIn(BaseModel):
    device: str | None = None
    slug: str
    seconds: int


def _device(db: Session, user: User, name: str) -> Device:
    name = (name or "").strip() or "windows-pc"
    dev = db.scalar(select(Device).where(Device.user_id == user.id, Device.name == name))
    if dev is None:
        dev = Device(user_id=user.id, name=name)
        db.add(dev)
    dev.last_seen = datetime.now(UTC).replace(tzinfo=None)
    return dev


@router.post("/hello")
def hello(body: HelloIn, db: Session = Depends(get_db), user: User = Depends(require_user)) -> dict:
    dev = _device(db, user, body.device)
    dev.platform = body.platform or "windows"
    dev.agent_version = body.agent_version
    db.commit()
    db.refresh(dev)
    return {"deviceId": dev.id, "user": user.username, "serverVersion": __version__}


@router.get("/games")
def agent_games(db: Session = Depends(get_db)) -> dict:
    """Compact game list for the agent (install method + download shape)."""
    games = db.scalars(select(Game).where(Game.missing.is_(False))).all()
    return {
        "games": [
            {
                "slug": g.slug,
                "title": g.title,
                "version": g.version,
                "kind": g.kind,
                "setupType": g.setup_type,
                "requiresHypervisor": g.requires_hypervisor,
                "sizeBytes": g.size_bytes,
                "downloadKind": DOWNLOAD_KIND.get(g.kind, "file"),
                "downloadName": download_name(g),
                "exeHint": g.exe_hint,
                "payloadPath": g.payload_path,
                "instructions": g.instructions,
            }
            for g in games
        ]
    }


@router.post("/installed")
def report_installed(
    body: InstalledIn, db: Session = Depends(get_db), user: User = Depends(require_user)
) -> dict:
    """Replace the device's installed-game set with what the agent reports."""
    dev = _device(db, user, body.device)
    db.flush()
    db.execute(delete(Installation).where(Installation.device_id == dev.id))
    slugs = {g.slug for g in body.games}
    by_slug = {g.slug: g for g in
               db.scalars(select(Game).where(Game.slug.in_(slugs)))} if slugs else {}
    for item in body.games:
        game = by_slug.get(item.slug)
        if game is None:
            continue
        db.add(Installation(device_id=dev.id, game_id=game.id, version=item.version,
                            install_path=item.install_path))
    db.commit()
    return {"ok": True, "count": len(by_slug)}


@router.post("/session")
def report_session(
    body: SessionIn, db: Session = Depends(get_db), user: User = Depends(require_user)
) -> dict:
    if body.seconds <= 0:
        return {"ok": True, "recorded": False}
    game = db.scalar(select(Game).where(Game.slug == body.slug))
    if game is None:
        raise HTTPException(404, "Game not found")
    dev = _device(db, user, body.device) if body.device else None
    db.flush()
    db.add(PlaySession(user_id=user.id, game_id=game.id,
                       device_id=dev.id if dev else None, seconds=body.seconds))
    db.commit()
    return {"ok": True, "recorded": True}

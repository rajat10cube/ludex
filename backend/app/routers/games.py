"""Browse the game library (read-only for any logged-in user)."""

from __future__ import annotations

from pathlib import Path

from fastapi import APIRouter, Depends, HTTPException, Query
from fastapi.responses import FileResponse
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from ..auth import require_user
from ..db import get_db
from ..models import Device, Game, Installation, PlaySession, User

router = APIRouter(prefix="/games", tags=["games"], dependencies=[Depends(require_user)])

# whether a folder game is downloaded as a streamed archive vs. a single file
DOWNLOAD_KIND = {"folder": "tar", "installer": "file", "archive": "file"}


def download_name(game: Game) -> str:
    """The filename the agent should save the download as."""
    if game.kind == "folder":
        return f"{game.slug}.tar"
    return Path(game.path).name


def serialize(game: Game, *, detail: bool = False, installed_versions: set | None = None,
              playtime: int = 0, last_played=None) -> dict:
    data = {
        "slug": game.slug,
        "title": game.title,
        "version": game.version,
        "kind": game.kind,
        "setupType": game.setup_type,
        "requiresHypervisor": game.requires_hypervisor,
        "releaseGroup": game.release_group,
        "sizeBytes": game.size_bytes,
        "fileCount": game.file_count,
        "hasCover": bool(game.cover_path),
        "missing": game.missing,
        "libraryId": game.library_id,
        "downloadKind": DOWNLOAD_KIND.get(game.kind, "file"),
        "downloadName": download_name(game),
        "installed": bool(installed_versions),
        "playtimeSeconds": playtime,
        "lastPlayed": last_played.isoformat() if last_played else None,
    }
    if detail:
        data.update({
            "description": game.description,
            "instructions": game.instructions,
            "exeHint": game.exe_hint,
            "payloadPath": game.payload_path,
            "installedVersions": sorted(installed_versions) if installed_versions else [],
        })
    return data


def _user_installed(db: Session, user_id: int) -> dict[int, set]:
    """game_id -> set of installed versions across the user's devices."""
    rows = db.execute(
        select(Installation.game_id, Installation.version)
        .join(Device, Device.id == Installation.device_id)
        .where(Device.user_id == user_id)
    ).all()
    out: dict[int, set] = {}
    for game_id, version in rows:
        out.setdefault(game_id, set()).add(version or "")
    return out


def _user_playtime(db: Session, user_id: int) -> dict[int, tuple[int, object]]:
    rows = db.execute(
        select(PlaySession.game_id, func.sum(PlaySession.seconds),
               func.max(PlaySession.played_at))
        .where(PlaySession.user_id == user_id)
        .group_by(PlaySession.game_id)
    ).all()
    return {gid: (int(total or 0), last) for gid, total, last in rows}


@router.get("")
def list_games(
    db: Session = Depends(get_db),
    user: User = Depends(require_user),
    search: str = "",
    library: int | None = None,
    sort: str = Query("title", pattern="^(title|size|recent)$"),
    include_missing: bool = False,
) -> dict:
    stmt = select(Game)
    if not include_missing:
        stmt = stmt.where(Game.missing.is_(False))
    if library is not None:
        stmt = stmt.where(Game.library_id == library)
    if search.strip():
        like = f"%{search.strip()}%"
        stmt = stmt.where(Game.title.ilike(like))
    if sort == "size":
        stmt = stmt.order_by(Game.size_bytes.desc())
    elif sort == "recent":
        stmt = stmt.order_by(Game.created_at.desc())
    else:
        stmt = stmt.order_by(func.lower(Game.title))

    games = db.scalars(stmt).all()
    installed = _user_installed(db, user.id)
    playtime = _user_playtime(db, user.id)
    return {
        "games": [
            serialize(
                g,
                installed_versions=installed.get(g.id),
                playtime=playtime.get(g.id, (0, None))[0],
                last_played=playtime.get(g.id, (0, None))[1],
            )
            for g in games
        ]
    }


@router.get("/{slug}")
def get_game(slug: str, db: Session = Depends(get_db), user: User = Depends(require_user)) -> dict:
    game = db.scalar(select(Game).where(Game.slug == slug))
    if game is None:
        raise HTTPException(404, "Game not found")
    installed = _user_installed(db, user.id).get(game.id)
    total, last = _user_playtime(db, user.id).get(game.id, (0, None))
    return serialize(game, detail=True, installed_versions=installed,
                     playtime=total, last_played=last)


@router.get("/{slug}/cover")
def game_cover(slug: str, db: Session = Depends(get_db)) -> FileResponse:
    game = db.scalar(select(Game).where(Game.slug == slug))
    if game is None or not game.cover_path:
        raise HTTPException(404, "No cover")
    p = Path(game.cover_path)
    if not p.is_file():
        raise HTTPException(404, "Cover file missing")
    return FileResponse(p)

"""Runtime library management (add/remove folders from the UI, like Jellyfin)."""

from __future__ import annotations

from pathlib import Path

from fastapi import APIRouter, BackgroundTasks, Depends, HTTPException
from pydantic import BaseModel
from sqlalchemy import delete, func, select
from sqlalchemy.orm import Session

from ..auth import require_admin
from ..db import get_db
from ..models import Game, Library
from ..scanner.service import run_scan, scan_status

router = APIRouter(prefix="/libraries", tags=["libraries"], dependencies=[Depends(require_admin)])


class LibraryIn(BaseModel):
    path: str
    name: str | None = None


@router.get("")
def list_libraries(db: Session = Depends(get_db)) -> list[dict]:
    rows = db.scalars(select(Library)).all()
    counts = dict(
        db.execute(
            select(Game.library_id, func.count())
            .where(Game.missing.is_(False))
            .group_by(Game.library_id)
        ).all()
    )
    return [
        {
            "id": lib.id,
            "path": lib.path,
            "name": lib.name,
            "gameCount": counts.get(lib.id, 0),
            "accessible": Path(lib.path).is_dir(),
        }
        for lib in rows
    ]


@router.post("", status_code=201)
def add_library(
    body: LibraryIn, background: BackgroundTasks, db: Session = Depends(get_db)
) -> dict:
    path = body.path.strip()
    if len(path) > 1:
        path = path.rstrip("/\\")
    if not path:
        raise HTTPException(400, "Path is required")
    if not Path(path).is_dir():
        raise HTTPException(400, f"Not a directory or not accessible inside the container: {path}")
    if db.scalar(select(Library).where(Library.path == path)):
        raise HTTPException(409, "That library already exists")

    lib = Library(path=path, name=(body.name or "").strip() or Path(path).name)
    db.add(lib)
    db.commit()
    db.refresh(lib)
    background.add_task(run_scan)  # scan the new folder in the background
    return {"id": lib.id, "path": lib.path, "name": lib.name}


@router.delete("/{library_id}", status_code=204)
def delete_library(library_id: int, db: Session = Depends(get_db)) -> None:
    if db.get(Library, library_id) is None:
        raise HTTPException(404, "Library not found")
    # ON DELETE CASCADE removes its games (and their installations/sessions)
    db.execute(delete(Library).where(Library.id == library_id))
    db.commit()


@router.post("/scan", status_code=202)
def trigger_scan(background: BackgroundTasks) -> dict:
    background.add_task(run_scan)
    return {"status": "started"}


@router.get("/scan/status")
def get_scan_status() -> dict:
    return scan_status()


@router.get("/browse")
def browse(path: str = "") -> dict:
    """List subdirectories of ``path`` (a built-in folder picker).

    An empty path lists filesystem roots (drive letters on Windows, ``/`` on POSIX).
    """
    if not path:
        return {"path": "", "parent": None, "dirs": _roots()}
    p = Path(path)
    if not p.is_dir():
        raise HTTPException(400, f"Not a directory: {path}")
    try:
        dirs = sorted(
            (c for c in p.iterdir() if c.is_dir() and not c.name.startswith(".")),
            key=lambda c: c.name.lower(),
        )
    except OSError as e:
        raise HTTPException(400, f"Cannot read {path}: {e}") from e
    parent = str(p.parent) if p != p.parent else ""
    return {
        "path": str(p),
        "parent": parent,
        "dirs": [{"name": c.name, "path": str(c)} for c in dirs[:2000]],
    }


def _roots() -> list[dict]:
    import string

    roots: list[dict] = []
    for letter in string.ascii_uppercase:
        drive = f"{letter}:\\"
        if Path(drive).is_dir():
            roots.append({"name": drive, "path": drive})
    if not roots:  # POSIX
        roots.append({"name": "/", "path": "/"})
    return roots

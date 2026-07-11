"""Scan library roots and sync Game rows.

Top-level entries of a library root become games:
  - a subfolder            -> kind "folder"    (classified into a setup method)
  - a loose .exe / .msi    -> kind "installer"
  - a loose .zip/.7z/.rar  -> kind "archive"
  - a loose .iso           -> kind "archive"   (setup_type "iso")
"""

from __future__ import annotations

import threading
from datetime import UTC, datetime
from pathlib import Path

from sqlalchemy import select

from ..config import get_settings
from ..db import SessionLocal
from ..models import Game, Library
from .classify import ISO_EXTS, classify_folder, find_cover
from .naming import clean_title, slugify

ARCHIVE_EXTS = {".zip", ".7z", ".rar"} | ISO_EXTS
INSTALLER_EXTS = {".exe", ".msi"}
IMAGE_EXTS = {".jpg", ".jpeg", ".png", ".webp"}
_MIN_FILE_BYTES = 64 * 1024  # ignore loose files smaller than this (junk)

_lock = threading.Lock()
_status: dict = {"state": "idle", "started_at": None, "finished_at": None,
                 "games": 0, "errors": []}


def scan_status() -> dict:
    return dict(_status)


def seed_libraries_from_config() -> None:
    """First-run import: copy configured library roots into the database."""
    with SessionLocal() as db:
        if db.scalar(select(Library.id).limit(1)) is not None:
            return
        for lib in get_settings().libraries():
            path = lib.path.rstrip("/\\") or lib.path
            db.add(Library(path=path, name=lib.name or Path(path).name))
        db.commit()


def _dir_stats(game_dir: Path) -> tuple[int, int]:
    total, count = 0, 0
    stack = [game_dir]
    while stack:
        folder = stack.pop()
        try:
            for entry in folder.iterdir():
                if entry.is_dir():
                    stack.append(entry)
                else:
                    try:
                        total += entry.stat().st_size
                        count += 1
                    except OSError:
                        pass
        except OSError:
            pass
    return total, count


def _collapse_single_child(folder: Path) -> Path:
    """Descend through redundant single-subfolder nesting (e.g. Crimson.Desert/Crimson.Desert).

    Stops as soon as a level has real files or more than one child directory.
    """
    current = folder
    for _ in range(3):
        try:
            children = [c for c in current.iterdir() if not c.name.startswith(".")]
        except OSError:
            break
        subdirs = [c for c in children if c.is_dir()]
        files = [c for c in children if c.is_file()]
        if len(children) == 1 and len(subdirs) == 1 and not files:
            current = subdirs[0]
        else:
            break
    return current


def _sibling_cover(file_path: Path) -> str | None:
    for ext in IMAGE_EXTS:
        candidate = file_path.with_suffix(ext)
        if candidate.is_file():
            return str(candidate)
    return None


def discover_games(root: Path) -> list[dict]:
    """Classify the top-level entries of a library root into game candidates."""
    settings = get_settings()
    found: list[dict] = []
    try:
        entries = sorted(root.iterdir(), key=lambda e: e.name.lower())
    except OSError:
        return found

    for entry in entries:
        if entry.name.startswith((".", "_", "@")):
            continue
        if entry.is_dir():
            title, version = clean_title(entry.name)
            inner = _collapse_single_child(entry)
            size, count = _dir_stats(entry) if settings.compute_sizes else (0, 0)
            if count == 0 and settings.compute_sizes:
                continue  # empty folder
            meta = classify_folder(inner, settings.exe_search_depth)
            # exe/payload hints are relative to `inner`; re-root them to `entry`
            prefix = inner.relative_to(entry).as_posix()
            if prefix and prefix != ".":
                for key in ("exe_hint", "payload_path"):
                    if meta[key]:
                        meta[key] = f"{prefix}/{meta[key]}"
            found.append({
                "path": str(entry), "kind": "folder", "title": title,
                "version": version, "cover_path": find_cover(inner) or find_cover(entry),
                "size_bytes": size, "file_count": count, **meta,
            })
        elif entry.is_file():
            ext = entry.suffix.lower()
            if ext in INSTALLER_EXTS:
                kind, setup_type = "installer", "installer"
            elif ext in ISO_EXTS:
                kind, setup_type = "archive", "iso"
            elif ext in ARCHIVE_EXTS:
                kind, setup_type = "archive", "archive"
            else:
                continue
            try:
                size = entry.stat().st_size
            except OSError:
                continue
            if size < _MIN_FILE_BYTES:
                continue
            title, version = clean_title(entry.stem)
            found.append({
                "path": str(entry), "kind": kind, "title": title, "version": version,
                "setup_type": setup_type, "requires_hypervisor": False,
                "cover_path": _sibling_cover(entry), "exe_hint": None,
                "payload_path": None, "instructions": None, "release_group": None,
                "size_bytes": size, "file_count": 1,
            })
    return found


# cover_path is handled separately so a re-scan never wipes a fetched cover.
_SYNC_FIELDS = (
    "title", "version", "kind", "setup_type", "requires_hypervisor", "release_group",
    "instructions", "exe_hint", "payload_path", "size_bytes", "file_count",
)


def _is_cached_cover(path: str | None) -> bool:
    """True if a cover path points into our artwork cache (a fetched cover)."""
    if not path:
        return False
    try:
        cache = get_settings().artwork_dir().resolve()
        return cache == Path(path).resolve().parent
    except OSError:
        return False


def _sync_library(db, library: Library) -> int:
    root = Path(library.path)
    now = datetime.now(UTC).replace(tzinfo=None)
    candidates = discover_games(root) if root.is_dir() else []
    existing = {g.path: g for g in
                db.scalars(select(Game).where(Game.library_id == library.id))}
    taken = set(db.scalars(select(Game.slug)))
    found_paths = set()

    for cand in candidates:
        found_paths.add(cand["path"])
        game = existing.get(cand["path"])
        if game is None:
            slug = slugify(cand["title"], cand["path"], taken)
            taken.add(slug)
            game = Game(slug=slug, library_id=library.id, **cand)
            db.add(game)
        else:
            for field in _SYNC_FIELDS:
                setattr(game, field, cand[field])
            # Prefer a local cover.jpg; otherwise keep any previously fetched cover.
            if cand["cover_path"]:
                game.cover_path = cand["cover_path"]
            elif not _is_cached_cover(game.cover_path):
                game.cover_path = None
            game.missing = False
        game.scanned_at = now

    for path, game in existing.items():
        if path not in found_paths:
            game.missing = True
    db.commit()
    return len(found_paths)


def backfill_artwork(force: bool = False) -> int:
    """Fetch covers + metadata for games that lack them. Returns count updated.

    Cheap and incremental: skips games that already have a cover/metadata, and
    skips ones with a recent miss marker unless ``force`` clears them first.
    A no-op when no provider keys are configured.
    """
    from ..artwork import ArtworkService

    art = ArtworkService()
    if not art.enabled():
        return 0
    if force:
        art.clear_markers()

    updated = 0
    with SessionLocal() as db:
        games = db.scalars(select(Game).where(Game.missing.is_(False))).all()
        for game in games:
            has_cover = bool(game.cover_path and Path(game.cover_path).is_file())
            want_cover = (art.sgdb is not None or art.igdb is not None) and not has_cover and (
                force or not (art.cache / f"{game.slug}.cover.miss").exists()
            )
            want_meta = art.igdb is not None and game.description is None and (
                force or not (art.cache / f"{game.slug}.meta.miss").exists()
            )
            if not (want_cover or want_meta):
                continue

            # Search by clean title only; version/edition tags hurt matching.
            cover_path, meta = art.fetch_for(
                game.slug, game.title, want_cover=want_cover, want_meta=want_meta
            )
            changed = False
            if cover_path is not None:
                game.cover_path = str(cover_path)
                changed = True
            if meta is not None:
                if meta.summary:
                    game.description = meta.summary
                if meta.genres:
                    game.genres = ", ".join(meta.genres)
                if meta.year:
                    game.release_year = meta.year
                if meta.rating is not None:
                    game.rating = meta.rating
                changed = True
            if changed:
                updated += 1
                db.commit()
    _status["artwork"] = updated
    return updated


def run_scan() -> dict:
    """Scan all libraries (serialized; safe to call from a background task)."""
    if not _lock.acquire(blocking=False):
        return scan_status()  # a scan is already running
    _status.update(state="scanning", errors=[], games=0, artwork=0,
                   started_at=datetime.now(UTC).isoformat(),
                   finished_at=None)
    try:
        total = 0
        with SessionLocal() as db:
            for library in db.scalars(select(Library)).all():
                try:
                    total += _sync_library(db, library)
                except Exception as e:  # keep scanning other libraries
                    db.rollback()
                    _status["errors"].append(f"{library.path}: {e}")
        _status["games"] = total
        try:
            backfill_artwork()
        except Exception as e:  # never let artwork failures break a scan
            _status["errors"].append(f"artwork: {e}")
    finally:
        _status.update(state="idle",
                       finished_at=datetime.now(UTC).isoformat())
        _lock.release()
    return scan_status()

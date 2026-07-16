"""Serve game payloads to the companion agent.

* **Folder games** are streamed as an uncompressed ``.tar`` (games are already
  compressed; store-mode avoids wasting CPU and needs no temp file). The Windows
  agent extracts them with the built-in ``tar.exe``.
* **Loose files** (``.zip`` / ``.iso`` / ``.exe`` / ``.msi``) are served directly
  with HTTP-range support so downloads can resume.
"""

from __future__ import annotations

import os
import queue
import tarfile
import threading
from pathlib import Path

from fastapi import APIRouter, Depends, Header, HTTPException, Request
from fastapi.responses import FileResponse, StreamingResponse
from sqlalchemy import select
from sqlalchemy.orm import Session

from ..auth import require_user
from ..db import get_db
from ..models import Game

router = APIRouter(prefix="/download", tags=["download"], dependencies=[Depends(require_user)])

_SKIP_DIRS = {"__macosx", "$recycle.bin", "system volume information"}


def _game_or_404(db: Session, slug: str) -> Game:
    game = db.scalar(select(Game).where(Game.slug == slug))
    if game is None:
        raise HTTPException(404, "Game not found")
    if game.missing or not Path(game.path).exists():
        raise HTTPException(410, "Game files are no longer available")
    return game


def _iter_tar(root: Path):
    """Stream a store-mode tar of ``root`` with backpressure (bounded memory)."""
    q: queue.Queue = queue.Queue(maxsize=8)
    sentinel = object()

    class _QWriter:
        def write(self, data: bytes) -> int:
            if data:
                q.put(bytes(data))
            return len(data)

    def worker() -> None:
        try:
            tar = tarfile.open(fileobj=_QWriter(), mode="w|", bufsize=1024 * 1024)
            for dirpath, dirnames, filenames in os.walk(root):
                # Deterministic order (both dirs and files) so a resumed download
                # produces byte-identical output and can continue from an offset.
                dirnames[:] = sorted(d for d in dirnames if d.lower() not in _SKIP_DIRS)
                for name in sorted(filenames):
                    full = Path(dirpath) / name
                    arcname = full.relative_to(root).as_posix()
                    # Open before writing anything: a file we can't read is then
                    # skipped cleanly, while no header is in the stream yet.
                    try:
                        fh = open(full, "rb")
                    except OSError:
                        continue
                    with fh:
                        try:
                            # stat the open fd, so the size we declare is the one
                            # we're about to read
                            info = tar.gettarinfo(str(full), arcname=arcname, fileobj=fh)
                        except Exception:
                            continue
                        if info is None or not info.isreg():
                            continue
                        try:
                            tar.addfile(info, fh)
                        except Exception as exc:
                            # addfile has already emitted this entry's header (and
                            # maybe part of its data). Skipping now would misalign
                            # every following entry and hand the client a corrupt
                            # archive, so fail loudly and name the file instead.
                            # Usual cause: the file is shorter than its stat size
                            # (e.g. still being written by a torrent client).
                            raise RuntimeError(
                                f"Couldn't read {arcname} while building the archive: {exc}"
                            ) from exc
            tar.close()
        except Exception as exc:  # surface to the consumer
            q.put(exc)
        finally:
            q.put(sentinel)

    threading.Thread(target=worker, daemon=True).start()
    while True:
        item = q.get()
        if item is sentinel:
            break
        if isinstance(item, Exception):
            raise item
        yield item


def _skip_prefix(gen, skip: int):
    """Drop the first ``skip`` bytes of a byte generator (for resuming a tar).

    The tar is deterministic, so a client that already has ``skip`` bytes can
    ask the server to regenerate and continue from there without re-sending them.
    """
    remaining = skip
    for chunk in gen:
        if remaining > 0:
            if len(chunk) <= remaining:
                remaining -= len(chunk)
                continue
            chunk = chunk[remaining:]
            remaining = 0
        yield chunk


def _ranged_file(path: Path, range_header: str | None):
    """FileResponse-style delivery with single-range support for resumable pulls."""
    file_size = path.stat().st_size
    if not range_header or not range_header.startswith("bytes="):
        return FileResponse(path, filename=path.name, media_type="application/octet-stream")

    try:
        start_s, _, end_s = range_header.removeprefix("bytes=").partition("-")
        start = int(start_s) if start_s else 0
        end = int(end_s) if end_s else file_size - 1
    except ValueError:
        raise HTTPException(416, "Invalid Range header") from None
    end = min(end, file_size - 1)
    if start > end or start >= file_size:
        raise HTTPException(416, "Requested Range Not Satisfiable")

    length = end - start + 1

    def stream():
        with open(path, "rb") as fh:
            fh.seek(start)
            remaining = length
            while remaining > 0:
                chunk = fh.read(min(1024 * 1024, remaining))
                if not chunk:
                    break
                remaining -= len(chunk)
                yield chunk

    headers = {
        "Content-Range": f"bytes {start}-{end}/{file_size}",
        "Accept-Ranges": "bytes",
        "Content-Length": str(length),
        "Content-Disposition": f'attachment; filename="{path.name}"',
    }
    return StreamingResponse(stream(), status_code=206, headers=headers,
                             media_type="application/octet-stream")


@router.get("/{slug}", response_model=None)
def download(
    slug: str,
    request: Request,
    db: Session = Depends(get_db),
    range_header: str | None = Header(default=None, alias="Range"),
    skip: int = 0,
) -> StreamingResponse | FileResponse:
    game = _game_or_404(db, slug)
    src = Path(game.path)
    if src.is_dir():
        # Folder games stream a deterministic tar; `skip` lets a paused download
        # resume by dropping the bytes the client already has (loose files below
        # use HTTP Range instead).
        gen = _iter_tar(src)
        if skip > 0:
            gen = _skip_prefix(gen, skip)
        headers = {
            "Content-Disposition": f'attachment; filename="{game.slug}.tar"',
            "Accept-Ranges": "none",
        }
        return StreamingResponse(gen, media_type="application/x-tar", headers=headers)
    return _ranged_file(src, range_header)

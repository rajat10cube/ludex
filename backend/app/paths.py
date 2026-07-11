"""Safe path resolution for downloads (path-traversal guard)."""

from __future__ import annotations

from pathlib import Path

from fastapi import HTTPException


def resolve_inside(root: Path, relpath: str) -> Path:
    """Resolve ``relpath`` under ``root``, rejecting anything that escapes it."""
    candidate = (root / relpath).resolve()
    root = root.resolve()
    if candidate != root and root not in candidate.parents:
        raise HTTPException(status_code=400, detail="Invalid path")
    return candidate

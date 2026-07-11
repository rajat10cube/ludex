"""Classify a downloaded/torrent game folder into an install method.

Real torrent repacks vary a lot. We recognise the common shapes:

* **Hypervisor / Denuvo** (DenuvOwO, "voksi"-style HV): a portable ``game\\...\\*.exe``
  plus a ``VBS.cmd`` and a "HOW TO USE" note about disabling Driver Signature
  Enforcement. These need manual VBS/DSE steps + a reboot, so we flag them.
* **ISO release**: the folder's real payload is a single big ``.iso`` (+ nfo/readme).
* **Installer**: a ``setup.exe`` / ``*.msi`` inside the folder.
* **Portable**: a runnable game ``.exe`` with no installer.
"""

from __future__ import annotations

import re
from pathlib import Path

IMAGE_EXTS = {".jpg", ".jpeg", ".png", ".webp"}
ISO_EXTS = {".iso", ".mds", ".mdf", ".nrg"}
INSTALLER_NAMES = ("setup", "install", "autorun")
INSTALLER_EXTS = {".exe", ".msi"}
NOTE_NAMES = (
    "how to use", "install tutorial", "readme", "read me", "instructions",
    "important", "how_to_install",
)
NOTE_EXTS = {".txt", ".nfo", ".md"}
# markers that a release uses a hypervisor / Denuvo-bypass and needs VBS/DSE off.
# Kept specific on purpose: short/loose tokens (e.g. a bare "f7") false-match hex
# hashes in .nfo files. The reliable signal is usually the VBS.cmd file itself.
_HV_MARKERS = re.compile(
    r"denuvowo|virtualization[- ]based security|driver signature enforcement|"
    r"disable driver signature|\bhypervisor\b|press f7|\bvbs\.cmd\b",
    re.IGNORECASE,
)
_HV_FILES = ("vbs.cmd", "denuvowo.nfo", "hv.cmd", "loader.cmd")
# executables that are never the game itself
_EXE_JUNK = (
    "unins", "setup", "install", "redist", "vcredist", "vc_redist", "dxsetup",
    "dxwebsetup", "dotnet", "oalinst", "crashhandler", "unitycrashhandler",
    "crashreport", "crashpad", "touchup", "cleanup", "activate", "register",
    "update", "launcher_installer", "easyanticheat_setup", "notification_helper",
)
_SKIP_DIRS = {"__macosx", "$recycle.bin", "system volume information"}
# folders that usually hold the real executable — searched/scored first
_GAME_DIR_HINTS = ("retail", "game", "bin", "binaries", "win64", "x64")


def _is_junk_exe(name: str) -> bool:
    low = name.lower()
    return any(tag in low for tag in _EXE_JUNK)


def iter_files(root: Path, max_depth: int):
    """Yield (path, depth) for files under root, skipping junk dirs."""
    stack: list[tuple[Path, int]] = [(root, 0)]
    while stack:
        folder, depth = stack.pop()
        try:
            entries = list(folder.iterdir())
        except OSError:
            continue
        for entry in entries:
            if entry.is_dir():
                if depth < max_depth and entry.name.lower() not in _SKIP_DIRS:
                    stack.append((entry, depth + 1))
            else:
                yield entry, depth


def _exe_score(path: Path, depth: int, size: int) -> int:
    """Higher = more likely to be the game's main executable."""
    score = min(size // (1024 * 1024), 4000)  # size in MB, capped
    parts = [p.lower() for p in path.parts]
    if any(hint in parts for hint in _GAME_DIR_HINTS):
        score += 3000
    score -= depth * 100  # shallower is slightly preferred among equals
    return score


def find_exe_hint(root: Path, max_depth: int) -> str | None:
    """Relative path of the most plausible main executable."""
    best: tuple[int, str] | None = None
    for entry, depth in iter_files(root, max_depth):
        if entry.suffix.lower() != ".exe" or _is_junk_exe(entry.name):
            continue
        try:
            size = entry.stat().st_size
        except OSError:
            continue
        score = _exe_score(entry, depth, size)
        rel = entry.relative_to(root).as_posix()
        if best is None or score > best[0]:
            best = (score, rel)
    return best[1] if best else None


def find_cover(root: Path) -> str | None:
    """A cover image at the top level (prefer names like cover/folder/boxart)."""
    try:
        images = [f for f in root.iterdir()
                  if f.is_file() and f.suffix.lower() in IMAGE_EXTS]
    except OSError:
        return None
    for stem in ("cover", "folder", "poster", "boxart", "grid"):
        for f in images:
            if f.stem.lower() == stem:
                return str(f)
    return str(images[0]) if images else None


def collect_instructions(root: Path, max_depth: int = 1, limit: int = 8000) -> str | None:
    """Concatenate the short human-readable notes (HOW TO USE / readme / nfo)."""
    chunks: list[str] = []
    for entry, _depth in iter_files(root, max_depth):
        if entry.suffix.lower() not in NOTE_EXTS:
            continue
        low = entry.name.lower()
        is_note = any(n in low for n in NOTE_NAMES) or entry.suffix.lower() == ".nfo"
        if not is_note:
            continue
        try:
            if entry.stat().st_size > 64 * 1024:
                continue
            text = entry.read_text(encoding="utf-8", errors="replace").strip()
        except OSError:
            continue
        if text:
            chunks.append(f"--- {entry.name} ---\n{text}")
    if not chunks:
        return None
    return ("\n\n".join(chunks))[:limit]


def detect_hypervisor(root: Path, instructions: str | None, max_depth: int = 2) -> bool:
    """True if the release needs a hypervisor + VBS/DSE disabled (Denuvo bypass)."""
    for entry, _ in iter_files(root, max_depth):
        if entry.name.lower() in _HV_FILES:
            return True
    if instructions and _HV_MARKERS.search(instructions):
        return True
    return False


def _release_group_from(name: str) -> str | None:
    m = re.search(r"[-.](voices\d+|fitgirl|dodi|codex|plaza|empress|rune|tenoke|"
                  r"skidrow|razor1911|elamigos|flt|goldberg|denuvowo)\b", name, re.IGNORECASE)
    return m.group(1) if m else None


def classify_folder(root: Path, exe_depth: int) -> dict:
    """Inspect a game folder and return setup metadata."""
    # An ISO release: the folder's payload is a disc image.
    iso = None
    for entry, _depth in iter_files(root, 2):
        if entry.suffix.lower() in ISO_EXTS:
            if iso is None or entry.stat().st_size > iso[1]:
                try:
                    iso = (entry, entry.stat().st_size)
                except OSError:
                    pass
    instructions = collect_instructions(root)
    hv = detect_hypervisor(root, instructions)
    group = _release_group_from(root.name)

    if iso is not None:
        return {
            "setup_type": "iso", "requires_hypervisor": hv,
            "exe_hint": None, "payload_path": iso[0].relative_to(root).as_posix(),
            "instructions": instructions, "release_group": group,
        }

    exe_hint = find_exe_hint(root, exe_depth)

    # An installer inside the folder (setup.exe / *.msi that isn't the game).
    installer_rel = None
    for entry, _depth in iter_files(root, 2):
        low = entry.stem.lower()
        if entry.suffix.lower() in INSTALLER_EXTS and any(n in low for n in INSTALLER_NAMES):
            installer_rel = entry.relative_to(root).as_posix()
            break

    if hv and exe_hint:
        setup_type = "portable_hypervisor"
    elif installer_rel and not exe_hint:
        setup_type = "installer"
    elif exe_hint:
        setup_type = "portable"
    elif installer_rel:
        setup_type = "installer"
    else:
        setup_type = "portable"  # unknown; agent will let the user browse

    return {
        "setup_type": setup_type, "requires_hypervisor": hv,
        "exe_hint": exe_hint, "payload_path": installer_rel,
        "instructions": instructions, "release_group": group,
    }

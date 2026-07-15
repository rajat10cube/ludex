"""Turn scene/repack-style folder and file names into clean titles.

``"Hades.v1.38299.Deluxe.[FitGirl.Repack]"`` → title ``"Hades Deluxe"``,
version ``"1.38299"``.
"""

from __future__ import annotations

import hashlib
import re

_RELEASE_TAGS = re.compile(
    r"\b(fitgirl|dodi|repack|goty|gog|codex|skidrow|plaza|empress|razor1911|elamigos|"
    r"tenoke|rune|flt|goldberg|denuvowo|reloaded|prophet|hoodlum|razordox|voices\d+|"
    r"multi\d*|x64|x86|win64|win32|64bit|"
    r"32bit|dlcs?([ ._-]included)?|selective|portable|setup|installer|update[ds]?|"
    r"cracked?|readnfo|rip)\b",
    re.IGNORECASE,
)
# "v1.2.3", "Version 1.2", "Build 12345" — and a bare dotted version at the end
_VERSION = re.compile(
    r"\b(?:v|version[ ._]?)(\d+(?:\.\d+)*[a-z0-9]*)\b|\bbuild[ ._-]?(\d{3,})\b",
    re.IGNORECASE,
)
_TRAILING_VERSION = re.compile(r"[ ._-](\d+(?:\.\d+){1,})\s*$")
_BRACKETS = re.compile(r"\[[^\]]*\]|\{[^}]*\}")
_PARENS = re.compile(r"\(([^)]*)\)")


def _drop_release_parens(match: re.Match) -> str:
    inner = match.group(1)
    if _RELEASE_TAGS.search(inner) or re.fullmatch(r"[\d .v]+", inner):
        return " "
    return match.group(0)  # keep meaningful parentheticals, e.g. "(Director's Cut)"


def clean_title(raw: str) -> tuple[str, str | None]:
    """Return ``(title, version)`` for a folder or file base name."""
    name = raw.strip()
    version: str | None = None

    m = _VERSION.search(name)
    if m:
        version = m.group(1) or m.group(2)
        name = name[: m.start()] + " " + name[m.end() :]

    name = _BRACKETS.sub(" ", name)
    name = _PARENS.sub(_drop_release_parens, name)

    if version is None:
        m = _TRAILING_VERSION.search(name)
        if m:
            version = m.group(1)
            name = name[: m.start()]

    # dotted/underscored scene names -> spaces (real titles rarely lack spaces)
    if " " not in name.strip():
        name = name.replace(".", " ").replace("_", " ")

    name = _RELEASE_TAGS.sub(" ", name)
    name = re.sub(r"[ ._-]+$", "", name)
    name = re.sub(r"^[ ._-]+", "", name)
    name = re.sub(r"\s{2,}", " ", name).strip(" -_")
    return (name or raw.strip()), version


def slugify(title: str, path: str, taken: set[str]) -> str:
    """URL-safe unique slug; disambiguated with a short path hash on collision."""
    base = re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-") or "game"
    if base not in taken:
        return base
    suffix = hashlib.md5(path.encode("utf-8", "surrogatepass")).hexdigest()[:6]
    return f"{base}-{suffix}"

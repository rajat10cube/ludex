"""Small persisted key/value store for runtime-editable settings (UI-managed).

Lives in ``<data_dir>/settings.json`` (outside the app dir, so it survives
updates). Currently holds the optional artwork provider keys, which take
precedence over the corresponding environment variables — so pasting a key in
the UI takes effect immediately, no restart.
"""

from __future__ import annotations

import json
import threading

from .config import get_settings

# Only these keys may be written through the store.
ALLOWED_KEYS = frozenset({"steamgriddb_key", "igdb_client_id", "igdb_client_secret"})

_lock = threading.Lock()


def _path():
    return get_settings().data_dir / "settings.json"


def load() -> dict:
    p = _path()
    if not p.exists():
        return {}
    try:
        data = json.loads(p.read_text(encoding="utf-8"))
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def get(key: str) -> str | None:
    return load().get(key) or None


def set_many(values: dict[str, str | None]) -> None:
    """Merge ``values`` into the store. Empty/None clears a key (falls back to env)."""
    with _lock:
        data = load()
        for key, value in values.items():
            if key not in ALLOWED_KEYS:
                continue
            if value:
                data[key] = value
            else:
                data.pop(key, None)
        p = _path()
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps(data, indent=2), encoding="utf-8")
        try:
            p.chmod(0o600)
        except OSError:
            pass

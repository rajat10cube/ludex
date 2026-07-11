"""Cover art + metadata providers (opt-in, like a TVDB key in Jellyfin).

* **SteamGridDB** supplies the portrait cover art shown in the library grid.
* **IGDB** (Twitch) supplies description / genres / release year / rating, and a
  cover as a fallback when SteamGridDB has none.

Both are free but need keys; with no keys configured everything here no-ops and
the scanner just uses a local ``cover.jpg`` if the game folder has one.

All network access goes through :func:`_http_json` / :func:`_download` so tests
can monkeypatch them without hitting the internet.
"""

from __future__ import annotations

import json
import time
import urllib.parse
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path

from .config import Settings, get_settings

SGDB_BASE = "https://www.steamgriddb.com/api/v2"
IGDB_TOKEN_URL = "https://id.twitch.tv/oauth2/token"
IGDB_GAMES_URL = "https://api.igdb.com/v4/games"
IGDB_IMAGE = "https://images.igdb.com/igdb/image/upload/t_cover_big/{image_id}.jpg"
_RATE_DELAY = 0.25  # seconds between network calls (be polite)


def _http_json(url: str, *, headers: dict | None = None, data: bytes | None = None,
               method: str = "GET", timeout: int = 15) -> dict:
    req = urllib.request.Request(url, data=data, headers=headers or {}, method=method)
    with urllib.request.urlopen(req, timeout=timeout) as resp:  # noqa: S310 (trusted hosts)
        return json.loads(resp.read().decode("utf-8"))


def _download(url: str, dest: Path, *, timeout: int = 15) -> None:
    req = urllib.request.Request(url, headers={"User-Agent": "Ludex"})
    with urllib.request.urlopen(req, timeout=timeout) as resp:  # noqa: S310
        dest.write_bytes(resp.read())


@dataclass
class Metadata:
    summary: str | None = None
    genres: list[str] = field(default_factory=list)
    year: int | None = None
    rating: int | None = None
    cover_url: str | None = None


class SteamGridDB:
    def __init__(self, key: str, timeout: int):
        self.key = key
        self.timeout = timeout

    @property
    def _headers(self) -> dict:
        return {"Authorization": f"Bearer {self.key}", "User-Agent": "Ludex"}

    def search_id(self, title: str) -> int | None:
        term = urllib.parse.quote(title)
        body = _http_json(f"{SGDB_BASE}/search/autocomplete/{term}",
                          headers=self._headers, timeout=self.timeout)
        data = body.get("data") or []
        return data[0]["id"] if data else None

    def cover_url(self, title: str) -> str | None:
        gid = self.search_id(title)
        if gid is None:
            return None
        # portrait grids first; fall back to any grid the game has
        for query in ("?dimensions=600x900&types=static&nsfw=false&limit=10",
                      "?types=static&nsfw=false&limit=10"):
            body = _http_json(f"{SGDB_BASE}/grids/game/{gid}{query}",
                              headers=self._headers, timeout=self.timeout)
            data = body.get("data") or []
            if data:
                return data[0].get("url")
        return None


class IGDB:
    def __init__(self, client_id: str, client_secret: str, timeout: int):
        self.client_id = client_id
        self.client_secret = client_secret
        self.timeout = timeout
        self._token: str | None = None
        self._token_expiry: float = 0.0

    def _ensure_token(self) -> str:
        if self._token and time.time() < self._token_expiry - 60:
            return self._token
        params = urllib.parse.urlencode({
            "client_id": self.client_id,
            "client_secret": self.client_secret,
            "grant_type": "client_credentials",
        })
        body = _http_json(f"{IGDB_TOKEN_URL}?{params}", method="POST", timeout=self.timeout)
        self._token = body["access_token"]
        self._token_expiry = time.time() + int(body.get("expires_in", 3600))
        return self._token

    def fetch(self, title: str) -> Metadata | None:
        token = self._ensure_token()
        headers = {
            "Client-ID": self.client_id,
            "Authorization": f"Bearer {token}",
            "Accept": "application/json",
        }
        # Apicalypse query. Escape quotes in the title.
        safe = title.replace('"', "")
        query = (f'search "{safe}"; '
                 "fields name,summary,genres.name,first_release_date,"
                 "total_rating,cover.image_id; limit 5;")
        rows = _http_json(IGDB_GAMES_URL, headers=headers, data=query.encode("utf-8"),
                          method="POST", timeout=self.timeout)
        if not rows:
            return None
        row = rows[0]
        meta = Metadata(summary=row.get("summary"))
        meta.genres = [g["name"] for g in row.get("genres", []) if g.get("name")]
        ts = row.get("first_release_date")
        if ts:
            meta.year = time.gmtime(ts).tm_year
        if row.get("total_rating") is not None:
            meta.rating = round(row["total_rating"])
        cover = row.get("cover") or {}
        if cover.get("image_id"):
            meta.cover_url = IGDB_IMAGE.format(image_id=cover["image_id"])
        return meta


class ArtworkService:
    """Orchestrates cover + metadata backfill with on-disk caching + miss markers."""

    def __init__(self, settings: Settings | None = None):
        s = settings or get_settings()
        self.timeout = s.artwork_timeout
        self.cache = s.artwork_dir()
        # UI-saved keys take precedence over environment variables.
        from . import settings_store

        sgdb_key = settings_store.get("steamgriddb_key") or s.steamgriddb_key
        igdb_id = settings_store.get("igdb_client_id") or s.igdb_client_id
        igdb_secret = settings_store.get("igdb_client_secret") or s.igdb_client_secret

        self.sgdb = SteamGridDB(sgdb_key, self.timeout) if sgdb_key else None
        self.igdb = (
            IGDB(igdb_id, igdb_secret, self.timeout)
            if (igdb_id and igdb_secret) else None
        )

    def enabled(self) -> bool:
        return bool(self.sgdb or self.igdb)

    def _cover_url(self, title: str) -> str | None:
        if self.sgdb:
            try:
                url = self.sgdb.cover_url(title)
                if url:
                    return url
            except Exception:  # noqa: BLE001 - provider hiccups shouldn't fail a scan
                pass
        return None

    def fetch_for(self, slug: str, title: str, *, want_cover: bool, want_meta: bool
                  ) -> tuple[Path | None, Metadata | None]:
        """Return (cover_path, metadata); either may be None. Writes miss markers."""
        cover_path: Path | None = None
        meta: Metadata | None = None

        igdb_meta: Metadata | None = None
        if want_meta and self.igdb:
            try:
                igdb_meta = self.igdb.fetch(title)
                time.sleep(_RATE_DELAY)
            except Exception:  # noqa: BLE001
                igdb_meta = None
            if igdb_meta:
                meta = igdb_meta
            else:
                (self.cache / f"{slug}.meta.miss").touch()

        if want_cover:
            url = self._cover_url(title)
            if not url and igdb_meta and igdb_meta.cover_url:
                url = igdb_meta.cover_url  # fall back to IGDB's cover
            if url:
                ext = _ext_from_url(url)
                dest = self.cache / f"{slug}{ext}"
                try:
                    _download(url, dest, timeout=self.timeout)
                    cover_path = dest
                    time.sleep(_RATE_DELAY)
                except Exception:  # noqa: BLE001
                    cover_path = None
            if cover_path is None:
                (self.cache / f"{slug}.cover.miss").touch()

        return cover_path, meta

    def clear_markers(self) -> None:
        for marker in self.cache.glob("*.miss"):
            marker.unlink(missing_ok=True)


def _ext_from_url(url: str) -> str:
    path = urllib.parse.urlparse(url).path.lower()
    for ext in (".png", ".webp", ".jpg", ".jpeg"):
        if path.endswith(ext):
            return ext
    return ".jpg"

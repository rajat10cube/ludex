"""Artwork/metadata providers, with all network access monkeypatched."""

from __future__ import annotations

from pathlib import Path

from app import artwork
from app.artwork import ArtworkService, _ext_from_url
from app.config import Settings


def _settings(tmp_path: Path, **kw) -> Settings:
    return Settings(data_dir=tmp_path / "data", **kw)


def test_ext_from_url():
    assert _ext_from_url("https://x/y/cover.png?token=1") == ".png"
    assert _ext_from_url("https://x/y/a.webp") == ".webp"
    assert _ext_from_url("https://x/y/no-extension") == ".jpg"


def test_disabled_without_keys(tmp_path):
    art = ArtworkService(_settings(tmp_path))
    assert art.enabled() is False
    assert art.sgdb is None and art.igdb is None


def test_steamgriddb_cover(tmp_path, monkeypatch):
    def fake_json(url, **_):
        if "/search/autocomplete/" in url:
            return {"data": [{"id": 42, "name": "Hades"}]}
        if "/grids/game/42" in url:
            return {"data": [{"url": "https://cdn.example/hades.png"}]}
        raise AssertionError(f"unexpected url {url}")

    def fake_download(url, dest, **_):
        assert url == "https://cdn.example/hades.png"
        Path(dest).write_bytes(b"PNGDATA")

    monkeypatch.setattr(artwork, "_http_json", fake_json)
    monkeypatch.setattr(artwork, "_download", fake_download)

    art = ArtworkService(_settings(tmp_path, steamgriddb_key="k"))
    assert art.enabled() and art.igdb is None
    cover, meta = art.fetch_for("hades", "Hades", want_cover=True, want_meta=True)
    assert cover is not None and cover.suffix == ".png" and cover.read_bytes() == b"PNGDATA"
    assert meta is None  # no IGDB configured


def test_igdb_metadata_and_cover_fallback(tmp_path, monkeypatch):
    def fake_json(url, **kwargs):
        if "oauth2/token" in url:
            return {"access_token": "tok", "expires_in": 3600}
        if url.endswith("/v4/games"):
            return [{
                "name": "Hades",
                "summary": "Rogue-like dungeon crawler.",
                "genres": [{"name": "Indie"}, {"name": "RPG"}],
                "first_release_date": 1600000000,  # 2020
                "total_rating": 92.4,
                "cover": {"image_id": "abc123"},
            }]
        raise AssertionError(f"unexpected url {url}")

    downloaded = {}

    def fake_download(url, dest, **_):
        downloaded["url"] = url
        Path(dest).write_bytes(b"IMG")

    monkeypatch.setattr(artwork, "_http_json", fake_json)
    monkeypatch.setattr(artwork, "_download", fake_download)

    art = ArtworkService(_settings(tmp_path, igdb_client_id="id", igdb_client_secret="s"))
    cover, meta = art.fetch_for("hades", "Hades", want_cover=True, want_meta=True)
    assert meta is not None
    assert meta.summary.startswith("Rogue-like")
    assert meta.genres == ["Indie", "RPG"]
    assert meta.year == 2020
    assert meta.rating == 92
    # no SteamGridDB -> cover falls back to IGDB's image
    assert cover is not None and downloaded["url"].endswith("abc123.jpg")


def test_miss_markers_written(tmp_path, monkeypatch):
    monkeypatch.setattr(artwork, "_http_json", lambda url, **k: {"data": []})
    art = ArtworkService(_settings(tmp_path, steamgriddb_key="k"))
    cover, _ = art.fetch_for("unknown", "Nope", want_cover=True, want_meta=False)
    assert cover is None
    assert (art.cache / "unknown.cover.miss").exists()
    art.clear_markers()
    assert not (art.cache / "unknown.cover.miss").exists()

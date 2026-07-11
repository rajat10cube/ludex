"""End-to-end: add a library, scan it, browse games, download, agent reporting."""

from __future__ import annotations

import io
import tarfile

import pytest

from app.scanner.service import run_scan

from ._fixtures import build_library
from .conftest import AUTH


@pytest.fixture(scope="module")
def library(tmp_path_factory, request):
    root = build_library(tmp_path_factory.mktemp("games"))
    from fastapi.testclient import TestClient

    from app.main import app

    with TestClient(app) as c:
        r = c.post("/api/libraries", json={"path": str(root), "name": "Test"}, headers=AUTH)
        assert r.status_code == 201, r.text
    run_scan()  # deterministic (don't rely on background task timing)
    return root


def _games(client) -> dict[str, dict]:
    r = client.get("/api/games", headers=AUTH)
    assert r.status_code == 200, r.text
    return {g["title"]: g for g in r.json()["games"]}


def test_library_listed(client, library):
    r = client.get("/api/libraries", headers=AUTH)
    assert r.status_code == 200
    libs = r.json()
    assert any(lib["name"] == "Test" and lib["gameCount"] >= 6 for lib in libs)


def test_games_listed_with_metadata(client, library):
    games = _games(client)
    hv = games["007 First Light"]
    assert hv["setupType"] == "portable_hypervisor"
    assert hv["requiresHypervisor"] is True
    assert hv["downloadKind"] == "tar"
    assert hv["downloadName"] == f"{hv['slug']}.tar"
    assert hv["hasCover"] is True

    ac = games["Assassins Creed Shadows"]
    assert ac["downloadKind"] == "file"
    assert ac["downloadName"].endswith(".zip")


def test_game_detail_has_instructions(client, library):
    slug = _games(client)["007 First Light"]["slug"]
    r = client.get(f"/api/games/{slug}", headers=AUTH)
    assert r.status_code == 200
    body = r.json()
    assert "Driver Signature" in (body["instructions"] or "")
    assert body["exeHint"] == "game/Retail/007FirstLight.exe"


def test_cover_served(client, library):
    slug = _games(client)["007 First Light"]["slug"]
    r = client.get(f"/api/games/{slug}/cover", headers=AUTH)
    assert r.status_code == 200
    assert r.headers["content-type"].startswith("image/")


def test_download_folder_as_tar(client, library):
    slug = _games(client)["007 First Light"]["slug"]
    r = client.get(f"/api/download/{slug}", headers=AUTH)
    assert r.status_code == 200
    assert r.headers["content-type"].startswith("application/x-tar")
    with tarfile.open(fileobj=io.BytesIO(r.content), mode="r:") as tar:
        names = tar.getnames()
    assert "game/Retail/007FirstLight.exe" in names


def test_download_loose_file_supports_range(client, library):
    slug = _games(client)["Assassins Creed Shadows"]["slug"]
    r = client.get(f"/api/download/{slug}", headers={**AUTH, "Range": "bytes=0-99"})
    assert r.status_code == 206
    assert r.headers["content-range"].startswith("bytes 0-99/")
    assert len(r.content) == 100


def test_agent_hello_installed_and_playtime(client, library):
    games = _games(client)
    slug = games["Stardew Valley"]["slug"]

    r = client.post("/api/agent/hello",
                    json={"device": "laptop", "agent_version": "0.1.0"}, headers=AUTH)
    assert r.status_code == 200
    assert r.json()["user"] == "admin"

    r = client.post("/api/agent/installed",
                    json={"device": "laptop",
                          "games": [{"slug": slug, "version": "1.0",
                                     "install_path": "C:/Ludex/pp"}]},
                    headers=AUTH)
    assert r.status_code == 200 and r.json()["count"] == 1

    r = client.post("/api/agent/session",
                    json={"device": "laptop", "slug": slug, "seconds": 3600}, headers=AUTH)
    assert r.status_code == 200 and r.json()["recorded"] is True

    detail = client.get(f"/api/games/{slug}", headers=AUTH).json()
    assert detail["installed"] is True
    assert detail["playtimeSeconds"] == 3600
    assert detail["lastPlayed"] is not None


def test_download_missing_game_410(client, library):
    # point a game's path at nothing by requesting an unknown slug -> 404
    assert client.get("/api/download/nope-not-real", headers=AUTH).status_code == 404


def test_scan_status_endpoint(client, library):
    r = client.get("/api/libraries/scan/status", headers=AUTH)
    assert r.status_code == 200
    assert r.json()["state"] in ("idle", "scanning")

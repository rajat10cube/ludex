from pathlib import Path

from app.scanner.service import discover_games

from ._fixtures import build_library


def _by_title(games: list[dict]) -> dict[str, dict]:
    return {g["title"]: g for g in games}


def test_discover_classifies_all_shapes(tmp_path: Path):
    root = build_library(tmp_path / "lib")
    games = _by_title(discover_games(root))

    # Hypervisor / Denuvo folder game
    hv = games["007 First Light"]
    assert hv["kind"] == "folder"
    assert hv["setup_type"] == "portable_hypervisor"
    assert hv["requires_hypervisor"] is True
    assert hv["exe_hint"] == "game/Retail/007FirstLight.exe"  # junk unins skipped
    assert hv["instructions"] and "Driver Signature" in hv["instructions"]
    assert hv["cover_path"] and hv["cover_path"].endswith("cover.jpg")

    # ISO-in-folder release
    iso = games["Mafia The Old Country"]
    assert iso["setup_type"] == "iso"
    assert iso["payload_path"].endswith(".iso")
    assert iso["release_group"] == "voices38"

    # Double-nested folder collapses; exe_hint is re-rooted to the top entry
    cd = games["Crimson Desert"]
    assert cd["setup_type"] == "portable_hypervisor"
    assert cd["exe_hint"] == "Crimson.Desert/game/Retail/CrimsonDesert.exe"

    # Plain portable
    pp = games["Stardew Valley"]
    assert pp["setup_type"] == "portable"
    assert pp["requires_hypervisor"] is False
    assert pp["exe_hint"] == "Stardew Valley.exe"

    # Split-RAR scene release: agent extracts, then re-classifies the output
    sm = games["Spider-Man Shattered Dimensions"]
    assert sm["kind"] == "folder"
    assert sm["setup_type"] == "rar"
    assert sm["payload_path"] == "rld-smsd.rar"  # first volume, not a .rNN part
    assert sm["release_group"] == "reloaded"

    # Loose archive + installer at the root
    ac = games["Assassins Creed Shadows"]
    assert ac["kind"] == "archive" and ac["setup_type"] == "archive"

    cool = games["Cool Game"]
    assert cool["kind"] == "installer" and cool["setup_type"] == "installer"


def test_missing_directory_is_safe(tmp_path: Path):
    assert discover_games(tmp_path / "does-not-exist") == []

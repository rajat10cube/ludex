"""Build a realistic fake game library on disk for scanner/API tests."""

from __future__ import annotations

from pathlib import Path

HOW_TO_USE = (
    "1. Run VBS.cmd as Administrator.\n"
    "   During restart press F7 and select 'Disable Driver Signature Enforcement'.\n"
    "2. Start the game from game\\Retail\\007FirstLight.exe as Administrator.\n"
)


def _write(path: Path, size: int = 1024, text: str | None = None) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if text is not None:
        path.write_text(text, encoding="utf-8")
    else:
        path.write_bytes(b"\0" * size)


def build_library(root: Path) -> Path:
    root.mkdir(parents=True, exist_ok=True)

    # 1) Hypervisor / DenuvOwO folder game
    hv = root / "007.First.Light"
    _write(hv / "VBS.cmd", text="@echo off\nrem disable VBS/DSE\n")
    _write(hv / "__HOW TO USE (PLEASE READ FIRST).txt", text=HOW_TO_USE)
    _write(hv / "DenuvOwO.nfo", text="DenuvOwO hypervisor release\n")
    _write(hv / "cover.jpg", size=2048)
    _write(hv / "game" / "Retail" / "007FirstLight.exe", size=300 * 1024)
    _write(hv / "game" / "Retail" / "unins000.exe", size=8 * 1024)  # junk, must be skipped

    # 2) ISO-in-folder release (voices38)
    iso = root / "Mafia.The.Old.Country-voices38"
    _write(iso / "voices38-mafia.the.old.country.iso", size=200 * 1024)
    _write(iso / "README.txt", text="Mount the ISO and run setup.\n")
    _write(iso / "voices38.nfo", text="voices38 iso release\n")

    # 3) Double-nested folder game (Crimson.Desert/Crimson.Desert/...)
    cd = root / "Crimson.Desert" / "Crimson.Desert"
    _write(cd / "VBS.cmd", text="@echo off\n")
    _write(cd / "game" / "Retail" / "CrimsonDesert.exe", size=250 * 1024)

    # 4) Plain portable folder game
    pg = root / "Stardew.Valley"
    _write(pg / "Stardew Valley.exe", size=120 * 1024)
    _write(pg / "Content.xnb", size=64 * 1024)

    # 5) Loose archive at the root
    _write(root / "Assassins.Creed.Shadows.zip", size=128 * 1024)

    # 6) Loose installer at the root
    _write(root / "Cool.Game.Setup.exe", size=128 * 1024)

    return root

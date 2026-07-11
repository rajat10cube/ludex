import pytest

from app.scanner.naming import clean_title, slugify


@pytest.mark.parametrize(
    "raw,title,version",
    [
        ("007.First.Light", "007 First Light", None),
        ("Mafia.The.Old.Country-voices38", "Mafia The Old Country", None),
        ("Hades.v1.38299.[FitGirl.Repack]", "Hades", "1.38299"),
        ("Crimson Desert (Director's Cut)", "Crimson Desert (Director's Cut)", None),
        ("Some.Game.Build.12345", "Some Game", "12345"),
        ("Cyberpunk 2077 v2.1", "Cyberpunk 2077", "2.1"),
        ("Assassins.Creed.Shadows", "Assassins Creed Shadows", None),
    ],
)
def test_clean_title(raw, title, version):
    got_title, got_version = clean_title(raw)
    assert got_title == title
    assert got_version == version


def test_slugify_unique():
    taken: set[str] = set()
    a = slugify("Crimson Desert", "/games/a", taken)
    taken.add(a)
    b = slugify("Crimson Desert", "/games/b", taken)
    assert a == "crimson-desert"
    assert b != a and b.startswith("crimson-desert-")

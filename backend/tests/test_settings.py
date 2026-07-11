"""UI-managed artwork keys: persistence, masking, and no-secret-echo."""

import pytest

from app import settings_store

from .conftest import AUTH


@pytest.fixture(autouse=True)
def _clean_store():
    yield
    # never leak test keys into other tests (they'd trigger real network calls)
    settings_store.set_many({k: None for k in settings_store.ALLOWED_KEYS})


def test_requires_admin(client):
    assert client.get("/api/settings/artwork").status_code == 401


def test_save_and_read_masked(client):
    secret_value = "abcd1234WXYZ9876"
    r = client.post("/api/settings/artwork",
                    json={"steamgriddb_key": secret_value}, headers=AUTH)
    assert r.status_code == 200
    body = r.json()
    # the raw key is never echoed back; only a masked hint + status
    assert secret_value not in str(body)
    assert body["steamgriddb_key"]["set"] is True
    assert body["steamgriddb_key"]["source"] == "saved"
    assert body["steamgriddb_key"]["hint"].endswith("9876")
    assert body["steamgriddb"] is True  # SteamGridDB now "connected"
    assert body["enabled"] is True


def test_igdb_needs_both_halves(client):
    r = client.post("/api/settings/artwork",
                    json={"igdb_client_id": "id123456"}, headers=AUTH).json()
    assert r["igdb_client_id"]["set"] is True
    assert r["igdb"] is False  # secret missing -> provider not enabled
    r = client.post("/api/settings/artwork",
                    json={"igdb_client_secret": "sec123456"}, headers=AUTH).json()
    assert r["igdb"] is True


def test_omitted_field_left_unchanged_then_cleared(client):
    client.post("/api/settings/artwork", json={"steamgriddb_key": "keykey1234"}, headers=AUTH)
    # posting an unrelated update leaves the sgdb key intact
    r = client.post("/api/settings/artwork", json={"igdb_client_id": "x"}, headers=AUTH).json()
    assert r["steamgriddb_key"]["set"] is True
    # explicit clear falls back to env (unset in tests) -> not set
    r = client.post("/api/settings/artwork",
                    json={"clear": ["steamgriddb_key"]}, headers=AUTH).json()
    assert r["steamgriddb_key"]["set"] is False

from .conftest import AUTH


def test_status_reports_ready(client):
    r = client.get("/api/auth/status")
    assert r.status_code == 200
    body = r.json()
    # preset admin means setup is already done
    assert body["needsSetup"] is False
    assert body["authDisabled"] is False


def test_me_requires_auth(client):
    assert client.get("/api/auth/me").status_code == 401


def test_me_with_basic(client):
    r = client.get("/api/auth/me", headers=AUTH)
    assert r.status_code == 200
    assert r.json()["username"] == "admin"
    assert r.json()["isAdmin"] is True


def test_login_bad_password(client):
    r = client.post("/api/auth/login", json={"username": "admin", "password": "nope"})
    assert r.status_code == 401


def test_games_require_auth(client):
    assert client.get("/api/games").status_code == 401

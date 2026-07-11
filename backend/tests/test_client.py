"""The client router serves the Windows agent scripts with the server URL baked in."""


def test_install_script_served_with_server_url(client):
    r = client.get("/api/client/install.ps1")
    assert r.status_code == 200
    assert "text/plain" in r.headers["content-type"]
    body = r.text
    assert "ludex://" in body
    assert "@@SERVER_URL@@" not in body  # placeholder was substituted
    assert "testserver" in body  # TestClient's base URL host


def test_agent_script_served(client):
    r = client.get("/api/client/ludex-agent.ps1")
    assert r.status_code == 200
    assert "Do-Install" in r.text
    assert "@@SERVER_URL@@" not in r.text


def test_unknown_script_404(client):
    assert client.get("/api/client/evil.ps1").status_code == 404

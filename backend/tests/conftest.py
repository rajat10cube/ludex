"""Test environment defaults (applied before the app is imported).

Seeds a known admin (admin/change-me) so Basic-auth tests work, isolates the DB
to a temp dir, and disables startup scanning. External env still wins (setdefault).
"""

import base64
import os
import tempfile

os.environ.pop("LUDEX_CONFIG", None)
os.environ.pop("LUDEX_GAMES_DIR", None)
os.environ.setdefault("LUDEX_DATA_DIR", tempfile.mkdtemp(prefix="ludex-test-"))
os.environ.setdefault("LUDEX_AUTH", "basic")
os.environ.setdefault("LUDEX_AUTH_USER", "admin")
os.environ.setdefault("LUDEX_AUTH_PASS", "change-me")
os.environ.setdefault("LUDEX_SCAN_ON_START", "false")

import pytest  # noqa: E402
from fastapi.testclient import TestClient  # noqa: E402

from app.db import init_db  # noqa: E402
from app.main import app  # noqa: E402

BASIC = "Basic " + base64.b64encode(b"admin:change-me").decode()
AUTH = {"Authorization": BASIC}


@pytest.fixture(scope="session", autouse=True)
def _bootstrap():
    init_db()  # create tables + seed the preset admin
    yield


@pytest.fixture
def client():
    with TestClient(app) as c:
        yield c

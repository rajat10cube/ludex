"""Application configuration.

Scalar settings come from environment variables (prefix ``LUDEX_``) and an
optional ``.env`` file. Library roots come from a YAML file (``LUDEX_CONFIG``)
or, as a single-root fallback, from ``LUDEX_GAMES_DIR``.
"""

from __future__ import annotations

from functools import lru_cache
from pathlib import Path

import yaml
from pydantic import BaseModel
from pydantic_settings import BaseSettings, SettingsConfigDict


class LibraryConfig(BaseModel):
    """One scanned library root."""

    path: str
    name: str | None = None


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_prefix="LUDEX_", env_file=".env", extra="ignore"
    )

    # --- storage ---
    config: str | None = None            # path to ludex.yaml (libraries)
    data_dir: Path = Path("./data")      # sqlite + secret key
    database_url: str | None = None

    # --- single-root fallback (used when no YAML config given) ---
    games_dir: str | None = None

    # --- scanning ---
    scan_on_start: bool = True
    exe_search_depth: int = 3            # how deep to look for a game's main .exe
    compute_sizes: bool = True           # sum folder sizes during scan (slower on huge libs)

    # --- auth ---
    auth: str = "basic"                  # none|basic (basic = require login)
    auth_user: str = "admin"
    auth_pass: str = ""                  # empty -> first-run signup creates the admin
    secret_key: str | None = None        # session signing key (auto-persisted if unset)

    # --- serving ---
    base_path: str = "/"
    dev_cors: bool = True                # allow the Vite dev origin (localhost:5173)
    client_dir: str | None = None        # override path to the Windows agent scripts

    def db_url(self) -> str:
        if self.database_url:
            return self.database_url
        self.data_dir.mkdir(parents=True, exist_ok=True)
        return f"sqlite:///{(self.data_dir / 'ludex.db').as_posix()}"

    def session_secret(self) -> str:
        """Stable signing key for session cookies (persisted so logins survive restarts)."""
        import secrets as _secrets

        if self.secret_key:
            return self.secret_key
        self.data_dir.mkdir(parents=True, exist_ok=True)
        f = self.data_dir / "secret.key"
        if f.exists():
            return f.read_text(encoding="utf-8").strip()
        value = _secrets.token_hex(32)
        f.write_text(value, encoding="utf-8")
        try:
            f.chmod(0o600)
        except OSError:
            pass
        return value

    def libraries(self) -> list[LibraryConfig]:
        if self.config:
            raw = yaml.safe_load(Path(self.config).read_text(encoding="utf-8")) or {}
            return [LibraryConfig(**lib) for lib in raw.get("libraries", [])]
        if self.games_dir:
            return [LibraryConfig(path=self.games_dir)]
        return []

    def client_scripts_dir(self) -> Path:
        """Where the Windows agent scripts live (repo `client/windows`)."""
        if self.client_dir:
            return Path(self.client_dir)
        # backend/app/config.py -> repo root is two levels above `backend/`
        return Path(__file__).resolve().parents[2] / "client" / "windows"


@lru_cache
def get_settings() -> Settings:
    return Settings()

"""Database engine, session, and SQLite pragmas."""

from __future__ import annotations

from collections.abc import Iterator

from sqlalchemy import create_engine, event
from sqlalchemy.orm import DeclarativeBase, Session, sessionmaker

from .config import get_settings


class Base(DeclarativeBase):
    pass


settings = get_settings()
engine = create_engine(
    settings.db_url(),
    connect_args={"check_same_thread": False},
    pool_pre_ping=True,
)


@event.listens_for(engine, "connect")
def _sqlite_pragmas(dbapi_conn, _record):  # noqa: ANN001
    cur = dbapi_conn.cursor()
    cur.execute("PRAGMA journal_mode=WAL")
    cur.execute("PRAGMA foreign_keys=ON")
    cur.execute("PRAGMA synchronous=NORMAL")
    cur.close()


SessionLocal = sessionmaker(bind=engine, autoflush=False, expire_on_commit=False)


def get_db() -> Iterator[Session]:
    db = SessionLocal()
    try:
        yield db
    finally:
        db.close()


def init_db() -> None:
    """Phase-0 bootstrap: create tables directly.

    Superseded by Alembic migrations once the first revision is generated
    (``alembic revision --autogenerate``). Safe to keep as a dev fallback.
    """
    from . import models  # noqa: F401  -- register models on Base.metadata

    Base.metadata.create_all(engine)
    _add_missing_columns()

    from .auth import ensure_admin  # seed the first admin from configured creds

    ensure_admin()


def _add_missing_columns() -> None:
    """Lightweight forward migration for pre-existing SQLite DBs.

    ``create_all`` never ALTERs an existing table, so new columns on Game are
    added here for installs created before this version.
    """
    additions = {
        "genres": "VARCHAR",
        "release_year": "INTEGER",
        "rating": "INTEGER",
    }
    with engine.begin() as conn:
        info = conn.exec_driver_sql("PRAGMA table_info(game)").fetchall()
        if not info:
            return  # fresh DB — create_all already made the full table
        existing = {row[1] for row in info}
        for name, sqltype in additions.items():
            if name not in existing:
                conn.exec_driver_sql(f"ALTER TABLE game ADD COLUMN {name} {sqltype}")

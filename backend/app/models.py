"""SQLAlchemy models — baseline schema."""

from __future__ import annotations

from datetime import datetime

from sqlalchemy import (
    BigInteger,
    Boolean,
    DateTime,
    ForeignKey,
    Integer,
    String,
    Text,
    UniqueConstraint,
    func,
)
from sqlalchemy.orm import Mapped, mapped_column, relationship

from .db import Base


class User(Base):
    __tablename__ = "user"

    id: Mapped[int] = mapped_column(primary_key=True)
    username: Mapped[str] = mapped_column(String, unique=True, index=True)
    password_hash: Mapped[str] = mapped_column(String)
    is_admin: Mapped[bool] = mapped_column(Boolean, default=False)
    created_at: Mapped[datetime] = mapped_column(DateTime, server_default=func.now())


class Library(Base):
    __tablename__ = "library"

    id: Mapped[int] = mapped_column(primary_key=True)
    path: Mapped[str] = mapped_column(String, unique=True)
    name: Mapped[str | None] = mapped_column(String, nullable=True)


class Game(Base):
    """One playable item found in a library root.

    kind (storage shape at the library root):
      folder    -- a game directory
      installer -- a loose setup .exe/.msi
      archive   -- a loose .zip/.7z/.rar/.iso

    setup_type (how the agent installs/runs it, torrent-repack aware):
      portable            -- extract/copy and run the exe directly
      portable_hypervisor -- portable, but Denuvo/hypervisor DRM: needs VBS.cmd +
                             Driver Signature Enforcement disabled first (manual)
      iso                 -- payload is an .iso: mount and run its setup
      installer           -- run a setup .exe/.msi
      archive             -- generic archive: extract, then look for a game
    """

    __tablename__ = "game"

    id: Mapped[int] = mapped_column(primary_key=True)
    slug: Mapped[str] = mapped_column(String, unique=True, index=True)
    title: Mapped[str] = mapped_column(String)
    version: Mapped[str | None] = mapped_column(String, nullable=True)
    description: Mapped[str | None] = mapped_column(Text, nullable=True)  # IGDB summary
    genres: Mapped[str | None] = mapped_column(String, nullable=True)      # comma-separated
    release_year: Mapped[int | None] = mapped_column(Integer, nullable=True)
    rating: Mapped[int | None] = mapped_column(Integer, nullable=True)     # 0-100 (IGDB)
    kind: Mapped[str] = mapped_column(String)  # folder|installer|archive
    setup_type: Mapped[str] = mapped_column(String, default="portable")
    requires_hypervisor: Mapped[bool] = mapped_column(Boolean, default=False)
    release_group: Mapped[str | None] = mapped_column(String, nullable=True)
    instructions: Mapped[str | None] = mapped_column(Text, nullable=True)  # HOW TO USE / nfo text
    library_id: Mapped[int | None] = mapped_column(
        ForeignKey("library.id", ondelete="CASCADE"), nullable=True
    )
    path: Mapped[str] = mapped_column(String, unique=True)
    cover_path: Mapped[str | None] = mapped_column(String, nullable=True)
    exe_hint: Mapped[str | None] = mapped_column(String, nullable=True)  # relpath of main exe
    payload_path: Mapped[str | None] = mapped_column(String, nullable=True)  # relpath of .iso/setup
    size_bytes: Mapped[int] = mapped_column(BigInteger, default=0)
    file_count: Mapped[int] = mapped_column(Integer, default=0)
    missing: Mapped[bool] = mapped_column(Boolean, default=False)
    created_at: Mapped[datetime] = mapped_column(DateTime, server_default=func.now())
    updated_at: Mapped[datetime] = mapped_column(
        DateTime, server_default=func.now(), onupdate=func.now()
    )
    scanned_at: Mapped[datetime | None] = mapped_column(DateTime, nullable=True)

    installations: Mapped[list[Installation]] = relationship(
        back_populates="game", cascade="all, delete-orphan"
    )


class Device(Base):
    """A machine running the companion agent (e.g. your Windows laptop)."""

    __tablename__ = "device"
    __table_args__ = (UniqueConstraint("user_id", "name", name="uq_device_user_name"),)

    id: Mapped[int] = mapped_column(primary_key=True)
    user_id: Mapped[int] = mapped_column(ForeignKey("user.id", ondelete="CASCADE"), index=True)
    name: Mapped[str] = mapped_column(String)
    platform: Mapped[str] = mapped_column(String, default="windows")
    agent_version: Mapped[str | None] = mapped_column(String, nullable=True)
    last_seen: Mapped[datetime] = mapped_column(
        DateTime, server_default=func.now(), onupdate=func.now()
    )

    installations: Mapped[list[Installation]] = relationship(
        back_populates="device", cascade="all, delete-orphan"
    )


class Installation(Base):
    """A game currently installed on a device (reported by the agent)."""

    __tablename__ = "installation"
    __table_args__ = (UniqueConstraint("device_id", "game_id", name="uq_install_device_game"),)

    id: Mapped[int] = mapped_column(primary_key=True)
    device_id: Mapped[int] = mapped_column(ForeignKey("device.id", ondelete="CASCADE"), index=True)
    game_id: Mapped[int] = mapped_column(ForeignKey("game.id", ondelete="CASCADE"), index=True)
    version: Mapped[str | None] = mapped_column(String, nullable=True)
    install_path: Mapped[str | None] = mapped_column(String, nullable=True)
    installed_at: Mapped[datetime] = mapped_column(DateTime, server_default=func.now())

    device: Mapped[Device] = relationship(back_populates="installations")
    game: Mapped[Game] = relationship(back_populates="installations")


class PlaySession(Base):
    """One play session reported by the agent (drives playtime + last-played)."""

    __tablename__ = "play_session"

    id: Mapped[int] = mapped_column(primary_key=True)
    user_id: Mapped[int] = mapped_column(ForeignKey("user.id", ondelete="CASCADE"), index=True)
    game_id: Mapped[int] = mapped_column(ForeignKey("game.id", ondelete="CASCADE"), index=True)
    device_id: Mapped[int | None] = mapped_column(
        ForeignKey("device.id", ondelete="SET NULL"), nullable=True
    )
    seconds: Mapped[int] = mapped_column(Integer, default=0)
    played_at: Mapped[datetime] = mapped_column(DateTime, server_default=func.now())

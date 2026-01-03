from datetime import datetime
from sqlalchemy import String, Boolean, DateTime, Integer, ARRAY, ForeignKey
from sqlalchemy.orm import Mapped, mapped_column, relationship

from app.models.base import Base


class User(Base):
    __tablename__ = "users"

    id: Mapped[int] = mapped_column(primary_key=True)
    email: Mapped[str] = mapped_column(String(255), unique=True, index=True)
    name: Mapped[str] = mapped_column(String(100))
    password_hash: Mapped[str] = mapped_column(String(255))
    created_at: Mapped[datetime] = mapped_column(DateTime, default=datetime.utcnow)

    preferences: Mapped["UserPreferences"] = relationship(
        back_populates="user", uselist=False
    )
    notes: Mapped[list["UserNote"]] = relationship(back_populates="user")
    listing_statuses: Mapped[list["UserListingStatus"]] = relationship(
        back_populates="user"
    )


class UserPreferences(Base):
    __tablename__ = "user_preferences"

    user_id: Mapped[int] = mapped_column(
        Integer, ForeignKey("users.id"), primary_key=True
    )
    min_price: Mapped[int | None] = mapped_column(Integer, nullable=True)
    max_price: Mapped[int | None] = mapped_column(Integer, nullable=True)
    min_bedrooms: Mapped[int | None] = mapped_column(Integer, nullable=True)
    min_sqft: Mapped[int | None] = mapped_column(Integer, nullable=True)
    cities: Mapped[list[str] | None] = mapped_column(ARRAY(String), nullable=True)
    property_types: Mapped[list[str] | None] = mapped_column(
        ARRAY(String), nullable=True
    )
    max_park_distance: Mapped[int | None] = mapped_column(Integer, nullable=True)
    notify_email: Mapped[bool] = mapped_column(Boolean, default=True)

    user: Mapped["User"] = relationship(back_populates="preferences")


# Forward references resolved at end of module
from app.models.note import UserNote, UserListingStatus  # noqa: E402, F401

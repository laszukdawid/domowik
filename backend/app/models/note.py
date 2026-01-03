from datetime import datetime
from sqlalchemy import Integer, ForeignKey, Text, DateTime, Boolean
from sqlalchemy.orm import Mapped, mapped_column, relationship

from app.models.base import Base


class UserNote(Base):
    __tablename__ = "user_notes"

    id: Mapped[int] = mapped_column(primary_key=True)
    listing_id: Mapped[int] = mapped_column(ForeignKey("listings.id"), index=True)
    user_id: Mapped[int] = mapped_column(ForeignKey("users.id"), index=True)
    note: Mapped[str] = mapped_column(Text)
    created_at: Mapped[datetime] = mapped_column(DateTime, default=datetime.utcnow)

    listing: Mapped["Listing"] = relationship(back_populates="notes")
    user: Mapped["User"] = relationship(back_populates="notes")


class UserListingStatus(Base):
    __tablename__ = "user_listing_status"

    user_id: Mapped[int] = mapped_column(ForeignKey("users.id"), primary_key=True)
    listing_id: Mapped[int] = mapped_column(ForeignKey("listings.id"), primary_key=True)
    is_favorite: Mapped[bool] = mapped_column(Boolean, default=False)
    is_hidden: Mapped[bool] = mapped_column(Boolean, default=False)
    viewed_at: Mapped[datetime | None] = mapped_column(DateTime, nullable=True)

    user: Mapped["User"] = relationship(back_populates="listing_statuses")
    listing: Mapped["Listing"] = relationship(back_populates="user_statuses")


from app.models.listing import Listing  # noqa: E402, F401
from app.models.user import User  # noqa: E402, F401

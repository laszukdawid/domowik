from datetime import datetime, UTC
from sqlalchemy import String, Integer, DateTime, Text, ForeignKey
from sqlalchemy.orm import Mapped, mapped_column, relationship

from app.models.base import Base


def utc_now() -> datetime:
    return datetime.now(UTC)


class CustomList(Base):
    __tablename__ = "custom_lists"

    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str] = mapped_column(Text, nullable=False)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utc_now)

    listings: Mapped[list["CustomListListing"]] = relationship(
        back_populates="custom_list", cascade="all, delete-orphan"
    )


class CustomListListing(Base):
    __tablename__ = "custom_list_listings"

    custom_list_id: Mapped[int] = mapped_column(
        ForeignKey("custom_lists.id", ondelete="CASCADE"), primary_key=True
    )
    listing_id: Mapped[int] = mapped_column(
        ForeignKey("listings.id", ondelete="CASCADE"), primary_key=True
    )
    source_url: Mapped[str | None] = mapped_column(Text, nullable=True)
    added_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utc_now)

    custom_list: Mapped["CustomList"] = relationship(back_populates="listings")
    listing: Mapped["Listing"] = relationship()


from app.models.listing import Listing  # noqa: E402, F401

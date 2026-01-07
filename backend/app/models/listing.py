from datetime import datetime, date, UTC
from sqlalchemy import String, Integer, DateTime, Date
from sqlalchemy.orm import Mapped, mapped_column, relationship
from sqlalchemy.dialects.postgresql import JSONB
from geoalchemy2 import Geometry

from app.models.base import Base


def utc_now() -> datetime:
    """Return current UTC time as timezone-aware datetime."""
    return datetime.now(UTC)


class Listing(Base):
    __tablename__ = "listings"

    id: Mapped[int] = mapped_column(primary_key=True)
    mls_id: Mapped[str] = mapped_column(String(50), unique=True, index=True)
    url: Mapped[str] = mapped_column(String(500))
    address: Mapped[str] = mapped_column(String(500))
    city: Mapped[str] = mapped_column(String(100), index=True)
    location = mapped_column(Geometry("POINT", srid=4326), nullable=True)
    price: Mapped[int] = mapped_column(Integer, index=True)
    bedrooms: Mapped[int | None] = mapped_column(Integer, nullable=True)
    bathrooms: Mapped[int | None] = mapped_column(Integer, nullable=True)
    sqft: Mapped[int | None] = mapped_column(Integer, nullable=True)
    property_type: Mapped[str | None] = mapped_column(String(50), nullable=True)
    listing_date: Mapped[date | None] = mapped_column(Date, nullable=True)
    first_seen: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utc_now)
    last_seen: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utc_now)
    status: Mapped[str] = mapped_column(String(20), default="active", index=True)
    raw_data: Mapped[dict | None] = mapped_column(JSONB, nullable=True)

    amenity_score: Mapped["AmenityScore"] = relationship(
        back_populates="listing", uselist=False
    )
    notes: Mapped[list["UserNote"]] = relationship(back_populates="listing")
    user_statuses: Mapped[list["UserListingStatus"]] = relationship(
        back_populates="listing"
    )
    poi_links: Mapped[list["ListingPOI"]] = relationship(
        "ListingPOI",
        primaryjoin="Listing.id == ListingPOI.listing_id",
        foreign_keys="ListingPOI.listing_id",
    )


# Forward references
from app.models.amenity import AmenityScore  # noqa: E402, F401
from app.models.note import UserNote, UserListingStatus  # noqa: E402, F401
from app.models.poi import ListingPOI  # noqa: E402, F401

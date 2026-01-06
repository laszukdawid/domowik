from sqlalchemy import BigInteger, String, Integer, DateTime, ForeignKey, func
from sqlalchemy.orm import Mapped, mapped_column, relationship
from geoalchemy2 import Geometry

from app.models.base import Base


class PointOfInterest(Base):
    __tablename__ = "points_of_interest"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    osm_id: Mapped[int] = mapped_column(BigInteger, unique=True, nullable=False, index=True)
    type: Mapped[str] = mapped_column(String(50), nullable=False, index=True)
    name: Mapped[str | None] = mapped_column(String(255), nullable=True)
    geometry: Mapped[str] = mapped_column(Geometry(srid=4326), nullable=False)
    created_at: Mapped[DateTime] = mapped_column(
        DateTime(timezone=True), server_default=func.now()
    )

    listing_links: Mapped[list["ListingPOI"]] = relationship(
        back_populates="poi", cascade="all, delete-orphan"
    )


class ListingPOI(Base):
    __tablename__ = "listing_pois"

    listing_id: Mapped[int] = mapped_column(
        Integer, ForeignKey("listings.id", ondelete="CASCADE"), primary_key=True
    )
    poi_id: Mapped[int] = mapped_column(
        Integer, ForeignKey("points_of_interest.id", ondelete="CASCADE"), primary_key=True
    )
    distance_m: Mapped[int] = mapped_column(Integer, nullable=False)

    poi: Mapped["PointOfInterest"] = relationship(back_populates="listing_links")

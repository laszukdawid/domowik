from sqlalchemy import Integer, ForeignKey
from sqlalchemy.orm import Mapped, mapped_column, relationship
from sqlalchemy.dialects.postgresql import JSONB

from app.models.base import Base


class AmenityScore(Base):
    __tablename__ = "amenity_scores"

    listing_id: Mapped[int] = mapped_column(
        ForeignKey("listings.id"), primary_key=True
    )
    nearest_park_m: Mapped[int | None] = mapped_column(Integer, nullable=True)
    nearest_coffee_m: Mapped[int | None] = mapped_column(Integer, nullable=True)
    nearest_dog_park_m: Mapped[int | None] = mapped_column(Integer, nullable=True)
    parks: Mapped[list | None] = mapped_column(JSONB, nullable=True)
    coffee_shops: Mapped[list | None] = mapped_column(JSONB, nullable=True)
    walkability_score: Mapped[int | None] = mapped_column(Integer, nullable=True)
    amenity_score: Mapped[int | None] = mapped_column(Integer, nullable=True)

    listing: Mapped["Listing"] = relationship(back_populates="amenity_score")


from app.models.listing import Listing  # noqa: E402, F401

from datetime import datetime, date
from pydantic import BaseModel


class AmenityScoreResponse(BaseModel):
    nearest_park_m: int | None
    nearest_coffee_m: int | None
    nearest_dog_park_m: int | None
    parks: list[dict] | None
    coffee_shops: list[dict] | None
    walkability_score: int | None
    amenity_score: int | None

    class Config:
        from_attributes = True


class ListingResponse(BaseModel):
    id: int
    mls_id: str
    url: str
    address: str
    city: str
    latitude: float | None
    longitude: float | None
    price: int
    bedrooms: int | None
    bathrooms: int | None
    sqft: int | None
    property_type: str | None
    listing_date: date | None
    first_seen: datetime
    status: str
    amenity_score: AmenityScoreResponse | None = None
    is_favorite: bool = False
    is_hidden: bool = False
    is_new: bool = False
    poi_ids: list[int] = []

    class Config:
        from_attributes = True


class ListingFilters(BaseModel):
    min_price: int | None = None
    max_price: int | None = None
    min_bedrooms: int | None = None
    min_sqft: int | None = None
    cities: list[str] | None = None
    property_types: list[str] | None = None
    max_park_distance: int | None = None
    include_hidden: bool = False
    favorites_only: bool = False


class FilterGroup(BaseModel):
    """A single filter group with AND conditions"""
    min_price: int | None = None
    max_price: int | None = None
    min_bedrooms: int | None = None
    min_sqft: int | None = None
    cities: list[str] | None = None
    property_types: list[str] | None = None
    min_score: int | None = None


class FilterGroups(BaseModel):
    """Multiple filter groups combined with OR logic"""
    groups: list[FilterGroup]
    include_hidden: bool = False
    favorites_only: bool = False
    polygons: list[list[list[float]]] | None = None  # List of polygons, each polygon is a list of [lng, lat] coordinates

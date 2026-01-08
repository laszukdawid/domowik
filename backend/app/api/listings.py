from typing import Annotated
from datetime import datetime, timedelta, UTC
import logging

from fastapi import APIRouter, Depends, HTTPException, Query
from fastapi.responses import StreamingResponse
from sqlalchemy import select, and_
from sqlalchemy.orm import selectinload
from sqlalchemy.ext.asyncio import AsyncSession
from geoalchemy2.functions import ST_X, ST_Y, ST_MakeEnvelope, ST_Within
import json

logger = logging.getLogger(__name__)

from app.models import get_db, Listing, AmenityScore, UserListingStatus, User
from app.api.deps import get_current_user
from app.schemas.listing import ListingResponse, AmenityScoreResponse

router = APIRouter(prefix="/listings", tags=["listings"])


def listing_to_response(
    listing: Listing,
    lng: float | None,
    lat: float | None,
    user_status: UserListingStatus | None,
    last_visit: datetime | None,
) -> ListingResponse:
    amenity = None
    if listing.amenity_score:
        amenity = AmenityScoreResponse.model_validate(listing.amenity_score)

    poi_ids = [link.poi_id for link in listing.poi_links] if listing.poi_links else []

    return ListingResponse(
        id=listing.id,
        mls_id=listing.mls_id,
        url=listing.url,
        address=listing.address,
        city=listing.city,
        latitude=lat,
        longitude=lng,
        price=listing.price,
        bedrooms=listing.bedrooms,
        bathrooms=listing.bathrooms,
        sqft=listing.sqft,
        property_type=listing.property_type,
        listing_date=listing.listing_date,
        first_seen=listing.first_seen,
        status=listing.status,
        amenity_score=amenity,
        is_favorite=user_status.is_favorite if user_status else False,
        is_hidden=user_status.is_hidden if user_status else False,
        is_new=last_visit is not None and listing.first_seen > last_visit,
        poi_ids=poi_ids,
    )


def build_listings_query(
    user: User,
    min_price: int | None = None,
    max_price: int | None = None,
    min_bedrooms: int | None = None,
    min_sqft: int | None = None,
    cities: list[str] | None = None,
    property_types: list[str] | None = None,
    include_hidden: bool = False,
    favorites_only: bool = False,
    min_score: int | None = None,
    bbox: str | None = None,  # "minLng,minLat,maxLng,maxLat"
):
    """Build the listings query with filters"""
    query = (
        select(
            Listing, UserListingStatus, ST_X(Listing.location), ST_Y(Listing.location)
        )
        .outerjoin(
            UserListingStatus,
            and_(
                UserListingStatus.listing_id == Listing.id,
                UserListingStatus.user_id == user.id,
            ),
        )
        .outerjoin(AmenityScore, AmenityScore.listing_id == Listing.id)
        .options(selectinload(Listing.amenity_score), selectinload(Listing.poi_links))
        .where(Listing.status == "active")
    )

    if min_price:
        query = query.where(Listing.price >= min_price)
    if max_price:
        query = query.where(Listing.price <= max_price)
    if min_bedrooms:
        query = query.where(Listing.bedrooms >= min_bedrooms)
    if min_sqft:
        query = query.where(Listing.sqft >= min_sqft)
    if cities:
        query = query.where(Listing.city.in_(cities))
    if property_types:
        query = query.where(Listing.property_type.in_(property_types))
    if not include_hidden:
        query = query.where(
            (UserListingStatus.is_hidden == False)  # noqa: E712
            | (UserListingStatus.is_hidden == None)  # noqa: E711
        )
    if favorites_only:
        query = query.where(UserListingStatus.is_favorite == True)  # noqa: E712
    if min_score is not None:
        query = query.where(AmenityScore.amenity_score >= min_score)

    if bbox:
        try:
            parts = bbox.split(",")
            if len(parts) != 4:
                raise ValueError(f"bbox must have exactly 4 components, got {len(parts)}")

            min_lng, min_lat, max_lng, max_lat = [float(x) for x in parts]

            # Validate coordinate bounds
            if not (-180 <= min_lng <= 180 and -180 <= max_lng <= 180):
                raise ValueError(
                    f"Longitude values must be within [-180, 180], got min_lng={min_lng}, max_lng={max_lng}"
                )
            if not (-90 <= min_lat <= 90 and -90 <= max_lat <= 90):
                raise ValueError(
                    f"Latitude values must be within [-90, 90], got min_lat={min_lat}, max_lat={max_lat}"
                )

            envelope = ST_MakeEnvelope(min_lng, min_lat, max_lng, max_lat, 4326)
            query = query.where(ST_Within(Listing.location, envelope))
        except (ValueError, AttributeError) as e:
            logger.warning(f"Invalid bbox parameter '{bbox}': {e}")

    query = query.order_by(AmenityScore.amenity_score.desc().nulls_last())
    return query


@router.get("/stream")
async def stream_listings(
    db: Annotated[AsyncSession, Depends(get_db)],
    user: Annotated[User, Depends(get_current_user)],
    min_price: int | None = None,
    max_price: int | None = None,
    min_bedrooms: int | None = None,
    min_sqft: int | None = None,
    cities: list[str] | None = Query(None),
    property_types: list[str] | None = Query(None),
    include_hidden: bool = False,
    favorites_only: bool = False,
    min_score: int | None = None,
    bbox: str | None = Query(
        None,
        description="Bounding box filter as 'minLng,minLat,maxLng,maxLat'. "
        "Longitude must be within [-180, 180], latitude within [-90, 90].",
    ),
    chunk_size: int = 25,  # Number of listings per chunk
):
    """Stream listings in chunks for progressive loading"""
    # Execute DB query BEFORE returning StreamingResponse to avoid connection leak.
    # FastAPI's dependency injection closes the session after the route returns,
    # but StreamingResponse's generator executes AFTER that - causing orphaned connections.
    query = build_listings_query(
        user, min_price, max_price, min_bedrooms, min_sqft,
        cities, property_types, include_hidden, favorites_only, min_score, bbox
    )

    result = await db.execute(query)
    rows = result.all()

    # Get user's last visit time
    last_visit = datetime.now(UTC) - timedelta(days=1)

    # Pre-convert all listings to response format while session is still valid
    listings_data = [
        listing_to_response(listing, lng, lat, status, last_visit).model_dump(mode='json')
        for listing, status, lng, lat in rows
    ]

    def generate_chunks():
        """Generator that chunks pre-fetched data (no DB access needed)"""
        chunk = []
        for item in listings_data:
            chunk.append(item)

            # Yield chunk when it reaches chunk_size
            if len(chunk) >= chunk_size:
                yield json.dumps(chunk) + "\n"
                chunk = []

        # Yield remaining items
        if chunk:
            yield json.dumps(chunk) + "\n"

    return StreamingResponse(
        generate_chunks(),
        media_type="application/x-ndjson",
        headers={
            "Cache-Control": "no-cache",
            "X-Accel-Buffering": "no",  # Disable nginx buffering
        }
    )


@router.get("", response_model=list[ListingResponse])
async def get_listings(
    db: Annotated[AsyncSession, Depends(get_db)],
    user: Annotated[User, Depends(get_current_user)],
    min_price: int | None = None,
    max_price: int | None = None,
    min_bedrooms: int | None = None,
    min_sqft: int | None = None,
    cities: list[str] | None = Query(None),
    property_types: list[str] | None = Query(None),
    include_hidden: bool = False,
    favorites_only: bool = False,
    min_score: int | None = None,
    bbox: str | None = Query(
        None,
        description="Bounding box filter as 'minLng,minLat,maxLng,maxLat'. "
        "Longitude must be within [-180, 180], latitude within [-90, 90].",
    ),
):
    """Get all listings at once (legacy endpoint)"""
    query = build_listings_query(
        user, min_price, max_price, min_bedrooms, min_sqft,
        cities, property_types, include_hidden, favorites_only, min_score, bbox
    )

    result = await db.execute(query)
    rows = result.all()

    # Get user's last visit time (simplified - track properly in production)
    last_visit = datetime.now(UTC) - timedelta(days=1)

    listings = []
    for listing, status, lng, lat in rows:
        resp = listing_to_response(listing, lng, lat, status, last_visit)
        listings.append(resp)

    return listings


@router.get("/{listing_id}", response_model=ListingResponse)
async def get_listing(
    listing_id: int,
    db: Annotated[AsyncSession, Depends(get_db)],
    user: Annotated[User, Depends(get_current_user)],
):
    query = (
        select(
            Listing, UserListingStatus, ST_X(Listing.location), ST_Y(Listing.location)
        )
        .outerjoin(
            UserListingStatus,
            and_(
                UserListingStatus.listing_id == Listing.id,
                UserListingStatus.user_id == user.id,
            ),
        )
        .options(selectinload(Listing.amenity_score), selectinload(Listing.poi_links))
        .where(Listing.id == listing_id)
    )

    result = await db.execute(query)
    row = result.first()

    if not row:
        raise HTTPException(status_code=404, detail="Listing not found")

    listing, status, lng, lat = row
    return listing_to_response(listing, lng, lat, status, None)

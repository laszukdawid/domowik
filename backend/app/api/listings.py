from typing import Annotated
from datetime import datetime, timedelta
from fastapi import APIRouter, Depends, HTTPException, Query
from sqlalchemy import select, and_
from sqlalchemy.orm import selectinload
from sqlalchemy.ext.asyncio import AsyncSession
from geoalchemy2.functions import ST_X, ST_Y

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
        .options(selectinload(Listing.amenity_score))
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

    query = query.order_by(Listing.first_seen.desc())

    result = await db.execute(query)
    rows = result.all()

    # Get user's last visit time (simplified - track properly in production)
    last_visit = datetime.utcnow() - timedelta(days=1)

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
        .options(selectinload(Listing.amenity_score))
        .where(Listing.id == listing_id)
    )

    result = await db.execute(query)
    row = result.first()

    if not row:
        raise HTTPException(status_code=404, detail="Listing not found")

    listing, status, lng, lat = row
    return listing_to_response(listing, lng, lat, status, None)

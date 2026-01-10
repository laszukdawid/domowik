import re
import logging
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, BackgroundTasks
from sqlalchemy import select, func
from sqlalchemy.ext.asyncio import AsyncSession

from app.models import get_db, Listing, CustomList, CustomListListing
from app.schemas.custom_list import (
    CustomListCreate,
    CustomListUpdate,
    CustomListResponse,
    AddListingRequest,
    AddListingResponse,
)
from scraper.realtor_ca import RealtorCaScraper
from scraper.enricher import AmenityEnricher

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/custom-lists", tags=["custom-lists"])


async def get_next_list_number(db: AsyncSession) -> int:
    """Get the next available list number for auto-naming."""
    result = await db.execute(
        select(func.count()).select_from(CustomList)
    )
    return result.scalar() + 1


def parse_mls_from_url(url: str) -> str | None:
    """Extract MLS ID from a realtor.ca URL.

    Handles formats like:
    - https://www.realtor.ca/real-estate/12345678/123-main-st-vancouver
    - https://realtor.ca/real-estate/12345678/...
    """
    # Match the numeric ID after /real-estate/
    match = re.search(r'realtor\.ca/real-estate/(\d+)', url)
    if match:
        return match.group(1)
    return None


def validate_mls_number(value: str) -> bool:
    """Check if value looks like a valid MLS number."""
    # MLS numbers are typically alphanumeric, 6-10 characters
    return bool(re.match(r'^[A-Za-z0-9]{6,15}$', value.strip()))


@router.get("", response_model=list[CustomListResponse])
async def get_custom_lists(
    db: Annotated[AsyncSession, Depends(get_db)],
):
    """Get all custom lists with their listing counts."""
    query = (
        select(
            CustomList.id,
            CustomList.name,
            CustomList.created_at,
            func.count(CustomListListing.listing_id).label("count"),
        )
        .outerjoin(CustomListListing, CustomList.id == CustomListListing.custom_list_id)
        .group_by(CustomList.id)
        .order_by(CustomList.created_at.desc())
    )

    result = await db.execute(query)
    rows = result.all()

    return [
        CustomListResponse(
            id=row.id,
            name=row.name,
            count=row.count,
            created_at=row.created_at,
        )
        for row in rows
    ]


@router.post("", response_model=CustomListResponse)
async def create_custom_list(
    db: Annotated[AsyncSession, Depends(get_db)],
    body: CustomListCreate | None = None,
):
    """Create a new custom list."""
    name = body.name if body and body.name else None

    if not name:
        # Auto-generate name
        num = await get_next_list_number(db)
        name = f"Custom Listing #{num}"

    custom_list = CustomList(name=name)
    db.add(custom_list)
    await db.commit()
    await db.refresh(custom_list)

    return CustomListResponse(
        id=custom_list.id,
        name=custom_list.name,
        count=0,
        created_at=custom_list.created_at,
    )


@router.patch("/{list_id}", response_model=CustomListResponse)
async def update_custom_list(
    list_id: int,
    body: CustomListUpdate,
    db: Annotated[AsyncSession, Depends(get_db)],
):
    """Rename a custom list."""
    result = await db.execute(
        select(CustomList).where(CustomList.id == list_id)
    )
    custom_list = result.scalar_one_or_none()

    if not custom_list:
        raise HTTPException(status_code=404, detail="List not found")

    custom_list.name = body.name
    await db.commit()
    await db.refresh(custom_list)

    # Get count
    count_result = await db.execute(
        select(func.count()).select_from(CustomListListing).where(
            CustomListListing.custom_list_id == list_id
        )
    )
    count = count_result.scalar()

    return CustomListResponse(
        id=custom_list.id,
        name=custom_list.name,
        count=count,
        created_at=custom_list.created_at,
    )


@router.delete("/{list_id}")
async def delete_custom_list(
    list_id: int,
    db: Annotated[AsyncSession, Depends(get_db)],
):
    """Delete a custom list."""
    result = await db.execute(
        select(CustomList).where(CustomList.id == list_id)
    )
    custom_list = result.scalar_one_or_none()

    if not custom_list:
        raise HTTPException(status_code=404, detail="List not found")

    await db.delete(custom_list)
    await db.commit()

    return {"status": "deleted"}


async def enrich_listing_background(listing_id: int):
    """Background task to enrich a listing with amenity data."""
    from app.models.base import async_session

    try:
        enricher = AmenityEnricher()
        async with async_session() as db:
            result = await db.execute(
                select(Listing).where(Listing.id == listing_id)
            )
            listing = result.scalar_one_or_none()
            if listing and listing.location:
                from geoalchemy2.functions import ST_X, ST_Y
                coord_result = await db.execute(
                    select(ST_X(Listing.location), ST_Y(Listing.location)).where(
                        Listing.id == listing_id
                    )
                )
                coords = coord_result.first()
                if coords:
                    lng, lat = coords
                    await enricher.enrich_listing(db, listing, lat, lng)
                    await db.commit()
        await enricher.close()
    except Exception as e:
        logger.error(f"Background enrichment failed for listing {listing_id}: {e}")


@router.post("/{list_id}/listings", response_model=AddListingResponse)
async def add_listing_to_list(
    list_id: int,
    body: AddListingRequest,
    background_tasks: BackgroundTasks,
    db: Annotated[AsyncSession, Depends(get_db)],
):
    """Add a listing to a custom list by URL or MLS number."""
    # Verify list exists
    list_result = await db.execute(
        select(CustomList).where(CustomList.id == list_id)
    )
    if not list_result.scalar_one_or_none():
        raise HTTPException(status_code=404, detail="List not found")

    input_value = body.input.strip()
    mls_id: str | None = None
    source_url: str | None = None

    # Determine if input is URL or MLS number
    if input_value.startswith("http://") or input_value.startswith("https://"):
        # Validate it's a realtor.ca URL
        if "realtor.ca" not in input_value.lower():
            raise HTTPException(
                status_code=400,
                detail="Only realtor.ca listing URLs are supported"
            )

        mls_id = parse_mls_from_url(input_value)
        if not mls_id:
            raise HTTPException(
                status_code=400,
                detail="Couldn't find listing ID in URL"
            )
        source_url = input_value
    else:
        # Treat as MLS number
        if not validate_mls_number(input_value):
            raise HTTPException(
                status_code=400,
                detail="Please enter a valid MLS number"
            )
        mls_id = input_value.upper()

    # Check if listing already exists in database
    result = await db.execute(
        select(Listing).where(Listing.mls_id == mls_id)
    )
    listing = result.scalar_one_or_none()
    status = "existing"

    if listing:
        # Listing exists, just add to list
        pass
    elif source_url:
        # Need to fetch from realtor.ca API
        scraper = RealtorCaScraper()
        try:
            scraped = await scraper.fetch_single(mls_id)
            if not scraped:
                raise HTTPException(
                    status_code=404,
                    detail="Listing not found - it may have been removed"
                )

            # Insert new listing
            from geoalchemy2.functions import ST_SetSRID, ST_MakePoint

            listing = Listing(
                mls_id=scraped.mls_id,
                url=scraped.url,
                address=scraped.address,
                city=scraped.city,
                location=ST_SetSRID(ST_MakePoint(scraped.longitude, scraped.latitude), 4326),
                price=scraped.price,
                bedrooms=scraped.bedrooms,
                bathrooms=scraped.bathrooms,
                sqft=scraped.sqft,
                property_type=scraped.property_type,
                listing_date=scraped.listing_date.date() if scraped.listing_date else None,
                raw_data=scraped.raw_data,
            )
            db.add(listing)
            await db.commit()
            await db.refresh(listing)
            status = "created"

            # Trigger background enrichment
            background_tasks.add_task(enrich_listing_background, listing.id)

        finally:
            await scraper.close()
    else:
        # MLS number only, but not in database
        raise HTTPException(
            status_code=404,
            detail="Listing not found. Try pasting the full realtor.ca URL instead."
        )

    # Check if already in this list
    existing_link = await db.execute(
        select(CustomListListing).where(
            CustomListListing.custom_list_id == list_id,
            CustomListListing.listing_id == listing.id,
        )
    )
    if not existing_link.scalar_one_or_none():
        # Add to list
        link = CustomListListing(
            custom_list_id=list_id,
            listing_id=listing.id,
            source_url=source_url,
        )
        db.add(link)
        await db.commit()

    return AddListingResponse(listing_id=listing.id, status=status)


@router.delete("/{list_id}/listings/{listing_id}")
async def remove_listing_from_list(
    list_id: int,
    listing_id: int,
    db: Annotated[AsyncSession, Depends(get_db)],
):
    """Remove a listing from a custom list."""
    result = await db.execute(
        select(CustomListListing).where(
            CustomListListing.custom_list_id == list_id,
            CustomListListing.listing_id == listing_id,
        )
    )
    link = result.scalar_one_or_none()

    if not link:
        raise HTTPException(status_code=404, detail="Listing not in this list")

    await db.delete(link)
    await db.commit()

    return {"status": "removed"}

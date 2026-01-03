"""
Main scraper runner script.

Run with: python -m scraper.run
"""

import asyncio
import sys
from datetime import datetime

from sqlalchemy import select
from sqlalchemy.ext.asyncio import create_async_engine, async_sessionmaker

from app.config import settings
from app.models import Listing, AmenityScore
from scraper.realtor_ca import RealtorCaScraper, ScrapedListing
from scraper.enricher import AmenityEnricher
from scraper.notifier import send_notifications


async def upsert_listing(session, scraped: ScrapedListing) -> tuple[Listing, bool]:
    """Insert or update a listing. Returns (listing, is_new)."""
    result = await session.execute(
        select(Listing).where(Listing.mls_id == scraped.mls_id)
    )
    existing = result.scalar_one_or_none()

    if existing:
        # Update existing listing
        existing.price = scraped.price
        existing.last_seen = datetime.utcnow()
        existing.raw_data = scraped.raw_data
        if existing.status == "delisted":
            existing.status = "active"
        return existing, False
    else:
        # Create new listing
        listing = Listing(
            mls_id=scraped.mls_id,
            url=scraped.url,
            address=scraped.address,
            city=scraped.city,
            price=scraped.price,
            bedrooms=scraped.bedrooms,
            bathrooms=scraped.bathrooms,
            sqft=scraped.sqft,
            property_type=scraped.property_type,
            listing_date=scraped.listing_date,
            raw_data=scraped.raw_data,
            status="active",
        )
        # Set location using WKT
        from sqlalchemy import text
        listing.location = text(
            f"ST_SetSRID(ST_MakePoint({scraped.longitude}, {scraped.latitude}), 4326)"
        )
        session.add(listing)
        return listing, True


async def enrich_listing(session, listing: Listing, enricher: AmenityEnricher):
    """Add amenity data to a listing."""
    # Get coordinates from the listing
    from sqlalchemy import text
    result = await session.execute(
        text(f"SELECT ST_X(location), ST_Y(location) FROM listings WHERE id = {listing.id}")
    )
    row = result.first()
    if not row or not row[0]:
        print(f"  No coordinates for listing {listing.mls_id}")
        return

    lng, lat = row

    # Check if already enriched
    existing = await session.get(AmenityScore, listing.id)
    if existing:
        print(f"  Already enriched: {listing.mls_id}")
        return

    print(f"  Enriching {listing.mls_id} at ({lat}, {lng})...")

    try:
        data = await enricher.enrich(lat, lng)

        amenity_score = AmenityScore(
            listing_id=listing.id,
            nearest_park_m=data.nearest_park_m,
            nearest_coffee_m=data.nearest_coffee_m,
            nearest_dog_park_m=data.nearest_dog_park_m,
            parks=data.parks,
            coffee_shops=data.coffee_shops,
            walkability_score=data.walkability_score,
            amenity_score=data.amenity_score,
        )
        session.add(amenity_score)
        print(f"  Score: {data.amenity_score}")

    except Exception as e:
        print(f"  Error enriching {listing.mls_id}: {e}")


async def mark_delisted(session, seen_mls_ids: set[str]):
    """Mark listings as delisted if not seen in scrape."""
    result = await session.execute(
        select(Listing).where(
            Listing.status == "active",
            ~Listing.mls_id.in_(seen_mls_ids),
        )
    )
    missing = result.scalars().all()

    for listing in missing:
        # Only delist after 2 consecutive misses
        hours_since_seen = (datetime.utcnow() - listing.last_seen).total_seconds() / 3600
        if hours_since_seen > 48:
            listing.status = "delisted"
            print(f"Delisted: {listing.mls_id}")


async def run_scraper():
    """Main scraper entry point."""
    print(f"Starting scraper at {datetime.utcnow()}")

    engine = create_async_engine(settings.database_url)
    async_session = async_sessionmaker(engine, expire_on_commit=False)

    scraper = RealtorCaScraper()
    enricher = AmenityEnricher()

    try:
        # Fetch all listings
        print("Fetching listings from realtor.ca...")
        scraped_listings = await scraper.fetch_all()
        print(f"Fetched {len(scraped_listings)} listings")

        if not scraped_listings:
            print("No listings fetched, exiting")
            return

        new_listings = []
        seen_mls_ids = set()

        async with async_session() as session:
            # Upsert listings
            print("Upserting listings...")
            for scraped in scraped_listings:
                seen_mls_ids.add(scraped.mls_id)
                listing, is_new = await upsert_listing(session, scraped)
                if is_new:
                    new_listings.append(listing)

            await session.commit()
            print(f"New listings: {len(new_listings)}")

            # Mark missing as delisted
            await mark_delisted(session, seen_mls_ids)
            await session.commit()

            # Enrich new listings
            print("Enriching new listings...")
            for listing in new_listings:
                await enrich_listing(session, listing, enricher)
                await session.commit()
                # Rate limit Overpass API
                await asyncio.sleep(1.0)

            # Send email notifications
            print("Sending notifications...")
            await send_notifications(session, new_listings)

        print(f"Scraper completed at {datetime.utcnow()}")

    finally:
        await scraper.close()
        await enricher.close()
        await engine.dispose()


if __name__ == "__main__":
    asyncio.run(run_scraper())

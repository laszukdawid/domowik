"""
Main scraper runner script.

Run with: python -m scraper.run
         python -m scraper.run --full  (for full weekly scrape)
"""

import argparse
import asyncio
import random
import sys
from datetime import datetime, UTC

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
        existing.last_seen = datetime.now(UTC)
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
    # Query for active listings not in the seen set
    # Use chunks to avoid issues with large IN clauses
    result = await session.execute(
        select(Listing).where(
            Listing.status == "active",
        )
    )
    active_listings = result.scalars().all()

    now = datetime.now(UTC)
    for listing in active_listings:
        if listing.mls_id in seen_mls_ids:
            continue
        # Only delist after 2 consecutive misses (48 hours)
        if listing.last_seen is None:
            continue
        hours_since_seen = (now - listing.last_seen).total_seconds() / 3600
        if hours_since_seen > 48:
            listing.status = "delisted"
            print(f"Delisted: {listing.mls_id}")


async def run_scraper(full_scrape: bool = False):
    """Main scraper entry point.

    Args:
        full_scrape: If True, fetch all pages. If False, stop when most listings
                     on a page already exist (incremental mode).
    """
    mode = "full" if full_scrape else "incremental"
    print(f"Starting {mode} scrape at {datetime.now(UTC)}")

    engine = create_async_engine(settings.database_url)
    async_session = async_sessionmaker(engine, expire_on_commit=False)

    scraper = RealtorCaScraper()
    enricher = AmenityEnricher()

    try:
        new_listings = []
        seen_mls_ids = set()
        total_fetched = 0

        # Fetch and upsert page by page so listings appear in UI progressively
        print("Fetching listings from realtor.ca...")
        page = 1
        total_pages = None

        async with async_session() as session:
            while True:
                # Fetch one page
                listings, total = await scraper.fetch_page(page)

                if page == 1:
                    if total == 0:
                        print("No listings found, exiting")
                        return
                    total_pages = min((total // 200) + 1, 100)
                    print(f"Page 1: {len(listings)} listings (total: {total})")
                else:
                    print(f"Page {page}/{total_pages}: {len(listings)} listings")

                # Upsert this page's listings immediately
                page_new_count = 0
                for scraped in listings:
                    seen_mls_ids.add(scraped.mls_id)
                    listing, is_new = await upsert_listing(session, scraped)
                    if is_new:
                        new_listings.append(listing)
                        page_new_count += 1

                # Commit after each page so listings appear in UI
                await session.commit()
                total_fetched += len(listings)

                # Check if done
                if page >= total_pages:
                    break

                # Incremental mode: stop if 80%+ of this page already existed
                if not full_scrape and len(listings) > 0:
                    existing_ratio = (len(listings) - page_new_count) / len(listings)
                    if existing_ratio >= 0.8:
                        print(f"Stopping early: {existing_ratio:.0%} of page {page} already existed")
                        break

                # Rate limiting before next page
                await asyncio.sleep(random.uniform(2.0, 3.0))
                page += 1

            print(f"Fetched {total_fetched} listings, {len(new_listings)} new")

            # Mark missing as delisted
            await mark_delisted(session, seen_mls_ids)
            await session.commit()

            # Enrich new listings
            print(f"Enriching {len(new_listings)} new listings...")
            for i, listing in enumerate(new_listings, 1):
                await enrich_listing(session, listing, enricher)
                await session.commit()
                if i % 10 == 0:
                    print(f"  Enriched {i}/{len(new_listings)} listings")

            # Send email notifications
            print("Sending notifications...")
            await send_notifications(session, new_listings)

        print(f"Scraper completed at {datetime.now(UTC)}")

    finally:
        await scraper.close()
        await enricher.close()
        await engine.dispose()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Scrape realtor.ca listings")
    parser.add_argument(
        "--full",
        action="store_true",
        help="Full scrape (all pages). Default is incremental (stop when caught up).",
    )
    args = parser.parse_args()
    asyncio.run(run_scraper(full_scrape=args.full))

"""
Enrich all listings that don't have amenity scores.

Run with: python -m scraper.enrich
"""

import asyncio
from datetime import datetime, UTC

from sqlalchemy import select, text
from sqlalchemy.ext.asyncio import create_async_engine, async_sessionmaker

from app.config import settings
from app.models import Listing, AmenityScore
from scraper.enricher import AmenityEnricher


async def enrich_listing(session, listing: Listing, enricher: AmenityEnricher) -> bool:
    """Add amenity data to a listing. Returns True if enriched successfully."""
    # Get coordinates from the listing
    result = await session.execute(
        text(f"SELECT ST_X(location), ST_Y(location) FROM listings WHERE id = {listing.id}")
    )
    row = result.first()
    if not row or not row[0]:
        print(f"  No coordinates for listing {listing.mls_id}")
        return False

    lng, lat = row

    print(f"  Enriching {listing.mls_id} at ({lat:.4f}, {lng:.4f})...")

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
        return True

    except Exception as e:
        print(f"  Error enriching {listing.mls_id}: {e}")
        return False


async def run_enrichment():
    """Enrich all listings that don't have amenity scores."""
    print(f"Starting enrichment at {datetime.now(UTC)}")

    engine = create_async_engine(settings.database_url)
    async_session = async_sessionmaker(engine, expire_on_commit=False)

    enricher = AmenityEnricher()

    try:
        async with async_session() as session:
            # Find listings without amenity scores
            result = await session.execute(
                select(Listing)
                .outerjoin(AmenityScore)
                .where(AmenityScore.listing_id == None)  # noqa: E711
                .where(Listing.status == "active")
                .order_by(Listing.first_seen.desc())
            )
            listings = result.scalars().all()

            print(f"Found {len(listings)} listings to enrich")

            if not listings:
                print("Nothing to enrich")
                return

            enriched = 0
            failed = 0

            for i, listing in enumerate(listings, 1):
                success = await enrich_listing(session, listing, enricher)
                if success:
                    enriched += 1
                else:
                    failed += 1

                await session.commit()

                if i % 10 == 0:
                    print(f"Progress: {i}/{len(listings)} ({enriched} enriched, {failed} failed)")

            print(f"\nEnrichment completed: {enriched} enriched, {failed} failed")

    finally:
        await enricher.close()
        await engine.dispose()

    print(f"Finished at {datetime.now(UTC)}")


if __name__ == "__main__":
    asyncio.run(run_enrichment())

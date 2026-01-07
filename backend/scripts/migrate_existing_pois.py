#!/usr/bin/env python
"""
Migrate existing POI data from amenity_scores JSONB to points_of_interest table.

The existing amenity_scores table stores parks and coffee_shops as JSONB arrays,
but these don't have OSM IDs needed for the new POI system. This script identifies
listings that need re-enrichment to populate the POI table with proper OSM data.

Run this once after deploying the new schema:
    cd backend && uv run python scripts/migrate_existing_pois.py
"""
import asyncio
from sqlalchemy import select
from sqlalchemy.ext.asyncio import create_async_engine, async_sessionmaker

from app.config import settings
from app.models.amenity import AmenityScore


async def migrate():
    engine = create_async_engine(settings.database_url)
    async_session = async_sessionmaker(engine, expire_on_commit=False)

    async with async_session() as db:
        # Get all amenity scores with POI data
        result = await db.execute(
            select(AmenityScore).where(
                (AmenityScore.parks.is_not(None)) |
                (AmenityScore.coffee_shops.is_not(None))
            )
        )
        scores = result.scalars().all()

        print(f"Found {len(scores)} listings with existing amenity data")

        listings_needing_reenrichment = []

        for score in scores:
            has_parks = score.parks and len(score.parks) > 0
            has_cafes = score.coffee_shops and len(score.coffee_shops) > 0

            # Check if any POI has osm_id (new format) vs just lat/lng (old format)
            parks_have_osm_id = has_parks and any(
                'osm_id' in p for p in score.parks
            )
            cafes_have_osm_id = has_cafes and any(
                'osm_id' in c for c in score.coffee_shops
            )

            if (has_parks and not parks_have_osm_id) or (has_cafes and not cafes_have_osm_id):
                listings_needing_reenrichment.append(score.listing_id)
                park_count = len(score.parks) if has_parks else 0
                cafe_count = len(score.coffee_shops) if has_cafes else 0
                print(f"  Listing {score.listing_id}: {park_count} parks, {cafe_count} cafes - needs re-enrichment")

        print(f"\nSummary:")
        print(f"  Total listings with amenity data: {len(scores)}")
        print(f"  Listings needing re-enrichment: {len(listings_needing_reenrichment)}")

        if listings_needing_reenrichment:
            print(f"\nTo re-enrich these listings, you can:")
            print(f"  1. Delete their amenity_scores records")
            print(f"  2. Run the enrichment script")
            print(f"\nOr run the enrichment script with --force to re-enrich all listings")

    await engine.dispose()


if __name__ == "__main__":
    asyncio.run(migrate())

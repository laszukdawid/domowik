"""Service for upserting POIs and creating listing links."""
import json
from sqlalchemy import select
from sqlalchemy.dialects.postgresql import insert
from sqlalchemy.ext.asyncio import AsyncSession
from geoalchemy2.functions import ST_GeomFromGeoJSON

from app.models.poi import PointOfInterest, ListingPOI


async def upsert_pois_for_listing(
    db: AsyncSession,
    listing_id: int,
    pois: list[dict],
) -> list[int]:
    """
    Upsert POIs and create listing links.

    Args:
        db: Database session
        listing_id: The listing to link POIs to
        pois: List of POI dicts with osm_id, name, type, geometry, distance_m

    Returns:
        List of POI IDs linked to the listing
    """
    if not pois:
        return []

    poi_ids = []

    for poi_data in pois:
        osm_id = poi_data["osm_id"]

        # Check if POI exists
        existing = await db.execute(
            select(PointOfInterest.id).where(PointOfInterest.osm_id == osm_id)
        )
        poi_id = existing.scalar_one_or_none()

        if poi_id is None:
            # Insert new POI
            geom_json = json.dumps(poi_data["geometry"])
            stmt = insert(PointOfInterest).values(
                osm_id=osm_id,
                type=poi_data["type"],
                name=poi_data.get("name"),
                geometry=ST_GeomFromGeoJSON(geom_json),
            ).returning(PointOfInterest.id)

            result = await db.execute(stmt)
            poi_id = result.scalar_one()

        poi_ids.append(poi_id)

        # Create listing-POI link (upsert to handle re-enrichment)
        link_stmt = insert(ListingPOI).values(
            listing_id=listing_id,
            poi_id=poi_id,
            distance_m=poi_data["distance_m"],
        ).on_conflict_do_update(
            index_elements=["listing_id", "poi_id"],
            set_={"distance_m": poi_data["distance_m"]},
        )
        await db.execute(link_stmt)

    return poi_ids

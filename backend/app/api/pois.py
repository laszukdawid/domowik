import json
from typing import Annotated
from fastapi import APIRouter, Depends, Query
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession
from geoalchemy2.functions import ST_AsGeoJSON

from app.models import get_db, User
from app.models.poi import PointOfInterest
from app.api.deps import get_current_user
from app.schemas.poi import POIResponse

router = APIRouter(prefix="/pois", tags=["pois"])


@router.get("", response_model=list[POIResponse])
async def get_pois(
    db: Annotated[AsyncSession, Depends(get_db)],
    user: Annotated[User, Depends(get_current_user)],
    ids: list[int] = Query(..., description="POI IDs to fetch"),
):
    """Fetch POI details by IDs."""
    if not ids:
        return []

    # Limit to prevent abuse
    if len(ids) > 100:
        ids = ids[:100]

    query = select(
        PointOfInterest.id,
        PointOfInterest.osm_id,
        PointOfInterest.type,
        PointOfInterest.name,
        ST_AsGeoJSON(PointOfInterest.geometry).label("geometry"),
    ).where(PointOfInterest.id.in_(ids))

    result = await db.execute(query)
    rows = result.all()

    return [
        POIResponse(
            id=row.id,
            osm_id=row.osm_id,
            type=row.type,
            name=row.name,
            geometry=json.loads(row.geometry),
        )
        for row in rows
    ]

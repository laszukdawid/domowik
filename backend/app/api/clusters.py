from typing import Annotated
import logging

from fastapi import APIRouter, Depends, Query
from sqlalchemy.ext.asyncio import AsyncSession
from sklearn.cluster import DBSCAN
import numpy as np

from app.models import get_db, Listing, User
from app.api.deps import get_current_user
from app.api.listings import build_listings_query

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/clusters", tags=["clusters"])


@router.get("")
async def get_clusters(
    db: Annotated[AsyncSession, Depends(get_db)],
    user: Annotated[User, Depends(get_current_user)],
    bbox: str = Query(
        ...,
        description="Bounding box filter as 'minLng,minLat,maxLng,maxLat'. "
        "Longitude must be within [-180, 180], latitude within [-90, 90].",
    ),
    zoom: int = 10,
    min_price: int | None = None,
    max_price: int | None = None,
    min_bedrooms: int | None = None,
    min_sqft: int | None = None,
    cities: list[str] | None = Query(None),
    property_types: list[str] | None = Query(None),
    include_hidden: bool = False,
    favorites_only: bool = False,
    min_score: int | None = None,
):
    """Return clustered listings for a viewport."""

    # Parse and validate bbox
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
    except (ValueError, AttributeError) as e:
        logger.warning(f"Invalid bbox parameter '{bbox}': {e}")
        return {"clusters": [], "outliers": []}

    # Fetch listings in bbox
    query = build_listings_query(
        user, min_price, max_price, min_bedrooms, min_sqft,
        cities, property_types, include_hidden, favorites_only, min_score, bbox
    )

    result = await db.execute(query)
    rows = result.all()

    if not rows:
        return {"clusters": [], "outliers": []}

    # If zoomed in enough or few listings, return as outliers (no clustering)
    if zoom >= 15 or len(rows) < 20:
        outliers = []
        for listing, status, lng, lat in rows:
            if lng and lat:
                outliers.append({
                    "id": listing.id,
                    "lat": lat,
                    "lng": lng,
                    "price": listing.price,
                    "bedrooms": listing.bedrooms,
                    "address": listing.address,
                    "amenity_score": listing.amenity_score.amenity_score if listing.amenity_score else None,
                    "is_favorite": status.is_favorite if status else False,
                })
        return {"clusters": [], "outliers": outliers}

    # Prepare data for clustering
    coords = []
    listing_data = []
    for listing, status, lng, lat in rows:
        if lng and lat:
            coords.append([lng, lat])
            listing_data.append({
                "listing": listing,
                "status": status,
                "lng": lng,
                "lat": lat,
            })

    if not coords:
        return {"clusters": [], "outliers": []}

    # Calculate eps based on zoom level (larger eps = bigger clusters)
    # At zoom 10, eps ~0.05 degrees (~5km), at zoom 14, eps ~0.005 (~500m)
    eps = 0.1 / (2 ** (zoom - 8))
    eps = max(0.001, min(0.2, eps))  # Clamp between 100m and 20km

    # Run DBSCAN clustering
    coords_array = np.array(coords)
    clustering = DBSCAN(eps=eps, min_samples=3, metric='euclidean').fit(coords_array)
    labels = clustering.labels_

    # Group by cluster
    cluster_map: dict[int, list] = {}
    outliers = []

    for idx, label in enumerate(labels):
        data = listing_data[idx]
        if label == -1:
            # Outlier (not in any cluster)
            outliers.append({
                "id": data["listing"].id,
                "lat": data["lat"],
                "lng": data["lng"],
                "price": data["listing"].price,
                "bedrooms": data["listing"].bedrooms,
                "address": data["listing"].address,
                "amenity_score": data["listing"].amenity_score.amenity_score if data["listing"].amenity_score else None,
                "is_favorite": data["status"].is_favorite if data["status"] else False,
            })
        else:
            if label not in cluster_map:
                cluster_map[label] = []
            cluster_map[label].append(data)

    # Build cluster summaries
    clusters = []
    for cluster_id, items in cluster_map.items():
        lats = [d["lat"] for d in items]
        lngs = [d["lng"] for d in items]
        prices = [d["listing"].price for d in items]
        beds = [d["listing"].bedrooms for d in items if d["listing"].bedrooms]
        scores = [d["listing"].amenity_score.amenity_score for d in items
                  if d["listing"].amenity_score and d["listing"].amenity_score.amenity_score]

        clusters.append({
            "id": f"cluster_{cluster_id}",
            "label": f"Area {cluster_id + 1}",
            "center": {
                "lat": sum(lats) / len(lats),
                "lng": sum(lngs) / len(lngs),
            },
            "bounds": {
                "north": max(lats),
                "south": min(lats),
                "east": max(lngs),
                "west": min(lngs),
            },
            "count": len(items),
            "stats": {
                "price_min": min(prices),
                "price_max": max(prices),
                "price_avg": int(sum(prices) / len(prices)),
                "beds_min": min(beds) if beds else None,
                "beds_max": max(beds) if beds else None,
                "amenity_avg": round(sum(scores) / len(scores), 1) if scores else None,
            },
            "listing_ids": [d["listing"].id for d in items],
        })

    # Sort clusters by count descending
    clusters.sort(key=lambda c: c["count"], reverse=True)

    return {"clusters": clusters, "outliers": outliers}

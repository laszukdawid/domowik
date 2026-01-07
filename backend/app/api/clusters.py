from typing import Annotated
import logging

from fastapi import APIRouter, Depends, Query
from sqlalchemy import select, and_, or_
from sqlalchemy.ext.asyncio import AsyncSession
from geoalchemy2.functions import ST_X, ST_Y, ST_MakeEnvelope, ST_Within, ST_MakePolygon, ST_MakeLine, ST_Point, ST_Contains
from sklearn.cluster import DBSCAN
import numpy as np

from app.models import get_db, Listing, AmenityScore, UserListingStatus, User
from app.api.deps import get_current_user
from app.schemas.listing import FilterGroup, FilterGroups

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/clusters", tags=["clusters"])


def build_lightweight_query(
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
    bbox: str | None = None,
):
    """Build a lightweight query for clustering - no eager loading of related objects."""
    query = (
        select(
            Listing.id,
            Listing.price,
            Listing.bedrooms,
            Listing.address,
            Listing.url,
            ST_X(Listing.location).label("lng"),
            ST_Y(Listing.location).label("lat"),
            UserListingStatus.is_favorite,
            AmenityScore.amenity_score,
        )
        .outerjoin(
            UserListingStatus,
            and_(
                UserListingStatus.listing_id == Listing.id,
                UserListingStatus.user_id == user.id,
            ),
        )
        .outerjoin(AmenityScore, AmenityScore.listing_id == Listing.id)
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
            if len(parts) == 4:
                min_lng, min_lat, max_lng, max_lat = [float(x) for x in parts]
                if (-180 <= min_lng <= 180 and -180 <= max_lng <= 180 and
                    -90 <= min_lat <= 90 and -90 <= max_lat <= 90):
                    envelope = ST_MakeEnvelope(min_lng, min_lat, max_lng, max_lat, 4326)
                    query = query.where(ST_Within(Listing.location, envelope))
        except (ValueError, AttributeError):
            pass

    return query


def build_lightweight_group_conditions(group: FilterGroup):
    """Build AND conditions for a single filter group (lightweight version)"""
    conditions = []

    if group.min_price:
        conditions.append(Listing.price >= group.min_price)
    if group.max_price:
        conditions.append(Listing.price <= group.max_price)
    if group.min_bedrooms:
        conditions.append(Listing.bedrooms >= group.min_bedrooms)
    if group.min_sqft:
        conditions.append(Listing.sqft >= group.min_sqft)
    if group.cities:
        conditions.append(Listing.city.in_(group.cities))
    if group.property_types:
        conditions.append(Listing.property_type.in_(group.property_types))
    if group.min_score is not None:
        conditions.append(AmenityScore.amenity_score >= group.min_score)

    return conditions


def build_lightweight_query_with_groups(
    user: User,
    filter_groups: list[FilterGroup],
    include_hidden: bool = False,
    favorites_only: bool = False,
    bbox: str | None = None,
    polygons: list[list[list[float]]] | None = None,
):
    """Build a lightweight query for clustering with OR filter groups and polygon filtering."""
    query = (
        select(
            Listing.id,
            Listing.price,
            Listing.bedrooms,
            Listing.address,
            Listing.url,
            ST_X(Listing.location).label("lng"),
            ST_Y(Listing.location).label("lat"),
            UserListingStatus.is_favorite,
            AmenityScore.amenity_score,
        )
        .outerjoin(
            UserListingStatus,
            and_(
                UserListingStatus.listing_id == Listing.id,
                UserListingStatus.user_id == user.id,
            ),
        )
        .outerjoin(AmenityScore, AmenityScore.listing_id == Listing.id)
        .where(Listing.status == "active")
    )

    # Build OR conditions from filter groups
    if filter_groups:
        group_conditions = []
        for group in filter_groups:
            conditions = build_lightweight_group_conditions(group)
            if conditions:
                # Combine conditions within a group with AND
                group_conditions.append(and_(*conditions))

        # Combine groups with OR
        if group_conditions:
            query = query.where(or_(*group_conditions))

    # Apply global filters
    if not include_hidden:
        query = query.where(
            (UserListingStatus.is_hidden == False)  # noqa: E712
            | (UserListingStatus.is_hidden == None)  # noqa: E711
        )
    if favorites_only:
        query = query.where(UserListingStatus.is_favorite == True)  # noqa: E712

    # Apply polygon filtering (properties must be within ANY of the polygons)
    if polygons:
        try:
            polygon_conditions = []
            for polygon_coords in polygons:
                if len(polygon_coords) < 3:
                    continue  # Need at least 3 points for a polygon

                # Ensure polygon is closed (first and last point are the same)
                if polygon_coords[0] != polygon_coords[-1]:
                    polygon_coords = polygon_coords + [polygon_coords[0]]

                # Create points from coordinates
                points = [ST_Point(lng, lat, 4326) for lng, lat in polygon_coords]

                # Create a linestring from the points
                linestring = ST_MakeLine(*points)

                # Create a polygon from the linestring
                polygon = ST_MakePolygon(linestring)

                # Add condition: listing location must be within this polygon
                polygon_conditions.append(ST_Contains(polygon, Listing.location))

            # Combine polygon conditions with OR (property can be in ANY polygon)
            if polygon_conditions:
                query = query.where(or_(*polygon_conditions))
        except Exception as e:
            logger.warning(f"Invalid polygon data: {e}")

    if bbox:
        try:
            parts = bbox.split(",")
            if len(parts) == 4:
                min_lng, min_lat, max_lng, max_lat = [float(x) for x in parts]
                if (-180 <= min_lng <= 180 and -180 <= max_lng <= 180 and
                    -90 <= min_lat <= 90 and -90 <= max_lat <= 90):
                    envelope = ST_MakeEnvelope(min_lng, min_lat, max_lng, max_lat, 4326)
                    query = query.where(ST_Within(Listing.location, envelope))
        except (ValueError, AttributeError):
            pass

    return query


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

    # Fetch listings in bbox using lightweight query
    query = build_lightweight_query(
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
        for row in rows:
            if row.lng and row.lat:
                outliers.append({
                    "id": row.id,
                    "lat": row.lat,
                    "lng": row.lng,
                    "price": row.price,
                    "bedrooms": row.bedrooms,
                    "address": row.address,
                    "url": row.url,
                    "amenity_score": row.amenity_score,
                    "is_favorite": row.is_favorite or False,
                })
        return {"clusters": [], "outliers": outliers}

    # Prepare data for clustering
    coords = []
    listing_data = []
    for row in rows:
        if row.lng and row.lat:
            coords.append([row.lng, row.lat])
            listing_data.append({
                "id": row.id,
                "price": row.price,
                "bedrooms": row.bedrooms,
                "address": row.address,
                "url": row.url,
                "amenity_score": row.amenity_score,
                "is_favorite": row.is_favorite or False,
                "lng": row.lng,
                "lat": row.lat,
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
                "id": data["id"],
                "lat": data["lat"],
                "lng": data["lng"],
                "price": data["price"],
                "bedrooms": data["bedrooms"],
                "address": data["address"],
                "url": data["url"],
                "amenity_score": data["amenity_score"],
                "is_favorite": data["is_favorite"],
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
        prices = [d["price"] for d in items]
        beds = [d["bedrooms"] for d in items if d["bedrooms"]]
        scores = [d["amenity_score"] for d in items if d["amenity_score"]]

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
            "listing_ids": [d["id"] for d in items],
        })

    # Sort clusters by count descending
    clusters.sort(key=lambda c: c["count"], reverse=True)

    return {"clusters": clusters, "outliers": outliers}


@router.post("/groups")
async def get_clusters_with_groups(
    filter_groups: FilterGroups,
    db: Annotated[AsyncSession, Depends(get_db)],
    user: Annotated[User, Depends(get_current_user)],
    bbox: str = Query(
        ...,
        description="Bounding box filter as 'minLng,minLat,maxLng,maxLat'. "
        "Longitude must be within [-180, 180], latitude within [-90, 90].",
    ),
    zoom: int = 10,
):
    """Return clustered listings for a viewport with OR filter groups."""

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

    # Fetch listings in bbox using lightweight query with groups
    query = build_lightweight_query_with_groups(
        user, filter_groups.groups, filter_groups.include_hidden,
        filter_groups.favorites_only, bbox, filter_groups.polygons
    )

    result = await db.execute(query)
    rows = result.all()

    if not rows:
        return {"clusters": [], "outliers": []}

    # If zoomed in enough or few listings, return as outliers (no clustering)
    if zoom >= 15 or len(rows) < 20:
        outliers = []
        for row in rows:
            if row.lng and row.lat:
                outliers.append({
                    "id": row.id,
                    "lat": row.lat,
                    "lng": row.lng,
                    "price": row.price,
                    "bedrooms": row.bedrooms,
                    "address": row.address,
                    "url": row.url,
                    "amenity_score": row.amenity_score,
                    "is_favorite": row.is_favorite or False,
                })
        return {"clusters": [], "outliers": outliers}

    # Prepare data for clustering
    coords = []
    listing_data = []
    for row in rows:
        if row.lng and row.lat:
            coords.append([row.lng, row.lat])
            listing_data.append({
                "id": row.id,
                "price": row.price,
                "bedrooms": row.bedrooms,
                "address": row.address,
                "url": row.url,
                "amenity_score": row.amenity_score,
                "is_favorite": row.is_favorite or False,
                "lng": row.lng,
                "lat": row.lat,
            })

    if not coords:
        return {"clusters": [], "outliers": []}

    # Calculate eps based on zoom level
    eps = 0.1 / (2 ** (zoom - 8))
    eps = max(0.001, min(0.2, eps))

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
                "id": data["id"],
                "lat": data["lat"],
                "lng": data["lng"],
                "price": data["price"],
                "bedrooms": data["bedrooms"],
                "address": data["address"],
                "url": data["url"],
                "amenity_score": data["amenity_score"],
                "is_favorite": data["is_favorite"],
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
        prices = [d["price"] for d in items]
        beds = [d["bedrooms"] for d in items if d["bedrooms"]]
        scores = [d["amenity_score"] for d in items if d["amenity_score"]]

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
            "listing_ids": [d["id"] for d in items],
        })

    # Sort clusters by count descending
    clusters.sort(key=lambda c: c["count"], reverse=True)

    return {"clusters": clusters, "outliers": outliers}

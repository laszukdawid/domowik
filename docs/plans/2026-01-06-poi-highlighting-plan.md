# POI Highlighting Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Display nearby points of interest (coffee shops, dog parks, parks) on the map when a listing is selected, with POIs stored as first-class entities shared across listings.

**Architecture:** New `PointOfInterest` table stores unique POIs with PostGIS geometry (points or polygons). `ListingPOI` junction table links listings to POIs with precomputed distances. Enricher upserts POIs by OSM ID, creates links. Frontend has LRU cache for POI objects, fetches missing ones on demand, renders at zoom >= 14.

**Tech Stack:** SQLAlchemy + GeoAlchemy2, Alembic migrations, FastAPI, React + Leaflet (react-leaflet), TypeScript

---

## Task 1: Create PointOfInterest Model

**Files:**
- Create: `backend/app/models/poi.py`
- Modify: `backend/app/models/__init__.py`

**Step 1: Write the POI model**

```python
# backend/app/models/poi.py
from sqlalchemy import BigInteger, String, Integer, DateTime, func
from sqlalchemy.orm import Mapped, mapped_column, relationship
from geoalchemy2 import Geometry

from app.models.base import Base


class PointOfInterest(Base):
    __tablename__ = "points_of_interest"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    osm_id: Mapped[int] = mapped_column(BigInteger, unique=True, nullable=False, index=True)
    type: Mapped[str] = mapped_column(String(50), nullable=False, index=True)
    name: Mapped[str | None] = mapped_column(String(255), nullable=True)
    geometry: Mapped[str] = mapped_column(Geometry(srid=4326), nullable=False)
    created_at: Mapped[DateTime] = mapped_column(
        DateTime(timezone=True), server_default=func.now()
    )

    listing_links: Mapped[list["ListingPOI"]] = relationship(
        back_populates="poi", cascade="all, delete-orphan"
    )


class ListingPOI(Base):
    __tablename__ = "listing_pois"

    listing_id: Mapped[int] = mapped_column(
        Integer, primary_key=True
    )
    poi_id: Mapped[int] = mapped_column(
        Integer, primary_key=True
    )
    distance_m: Mapped[int] = mapped_column(Integer, nullable=False)

    poi: Mapped["PointOfInterest"] = relationship(back_populates="listing_links")


from app.models.poi import ListingPOI  # noqa: E402, F401
```

**Step 2: Update models __init__.py**

Add to `backend/app/models/__init__.py`:

```python
from app.models.poi import PointOfInterest, ListingPOI
```

And add to `__all__`:

```python
__all__ = [..., "PointOfInterest", "ListingPOI"]
```

**Step 3: Verify model imports**

Run: `cd /home/dawid/projects/domowik/backend && uv run python -c "from app.models import PointOfInterest, ListingPOI; print('OK')"`
Expected: `OK`

**Step 4: Commit**

```bash
git add backend/app/models/poi.py backend/app/models/__init__.py
git commit -m "feat(models): add PointOfInterest and ListingPOI models"
```

---

## Task 2: Create Alembic Migration

**Files:**
- Create: `backend/alembic/versions/XXXX_add_poi_tables.py`

**Step 1: Generate migration**

Run: `cd /home/dawid/projects/domowik/backend && uv run alembic revision -m "add_poi_tables"`

**Step 2: Edit migration file**

```python
"""add_poi_tables

Revision ID: <generated>
Revises: e1d361bc305e
Create Date: <generated>
"""
from typing import Sequence, Union
from alembic import op
import sqlalchemy as sa
import geoalchemy2

revision: str = "<generated>"
down_revision: Union[str, None] = "e1d361bc305e"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.create_table(
        "points_of_interest",
        sa.Column("id", sa.Integer(), nullable=False),
        sa.Column("osm_id", sa.BigInteger(), nullable=False),
        sa.Column("type", sa.String(50), nullable=False),
        sa.Column("name", sa.String(255), nullable=True),
        sa.Column("geometry", geoalchemy2.Geometry(srid=4326), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.func.now()),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index("ix_poi_osm_id", "points_of_interest", ["osm_id"], unique=True)
    op.create_index("ix_poi_type", "points_of_interest", ["type"])
    op.create_index("ix_poi_geometry", "points_of_interest", ["geometry"], postgresql_using="gist")

    op.create_table(
        "listing_pois",
        sa.Column("listing_id", sa.Integer(), nullable=False),
        sa.Column("poi_id", sa.Integer(), nullable=False),
        sa.Column("distance_m", sa.Integer(), nullable=False),
        sa.ForeignKeyConstraint(["listing_id"], ["listings.id"], ondelete="CASCADE"),
        sa.ForeignKeyConstraint(["poi_id"], ["points_of_interest.id"], ondelete="CASCADE"),
        sa.PrimaryKeyConstraint("listing_id", "poi_id"),
    )
    op.create_index("ix_listing_pois_listing", "listing_pois", ["listing_id"])
    op.create_index("ix_listing_pois_poi", "listing_pois", ["poi_id"])


def downgrade() -> None:
    op.drop_table("listing_pois")
    op.drop_table("points_of_interest")
```

**Step 3: Run migration**

Run: `cd /home/dawid/projects/domowik && task backend:migrate`
Expected: Migration applies successfully

**Step 4: Verify tables exist**

Run: `docker exec -it domowik-db-1 psql -U postgres -d housesearch -c "\dt *poi*"`
Expected: Shows `points_of_interest` and `listing_pois` tables

**Step 5: Commit**

```bash
git add backend/alembic/versions/*_add_poi_tables.py
git commit -m "feat(db): add POI tables migration"
```

---

## Task 3: Update Enricher to Store POI Geometry

**Files:**
- Modify: `backend/scraper/enricher.py`

**Step 1: Update Overpass queries to fetch geometry**

Modify `get_nearby_parks` to use `out body geom;` instead of `out center;`:

```python
async def get_nearby_parks(
    self, lat: float, lng: float, radius_m: int = 1000
) -> list[dict]:
    """Get parks within radius with full geometry."""
    query = f"""
    [out:json][timeout:10];
    (
      way["leisure"="park"](around:{radius_m},{lat},{lng});
      relation["leisure"="park"](around:{radius_m},{lat},{lng});
      way["leisure"="garden"](around:{radius_m},{lat},{lng});
      way["leisure"="playground"](around:{radius_m},{lat},{lng});
    );
    out body geom;
    """

    elements = await self._query_overpass(query)
    parks = []

    for el in elements:
        osm_id = el.get("id")
        el_type = el.get("type")

        # Extract geometry
        geometry = None
        centroid_lat, centroid_lng = None, None

        if el_type == "way" and "geometry" in el:
            # Way with geometry - build polygon coordinates
            coords = [[pt["lon"], pt["lat"]] for pt in el["geometry"]]
            if coords and coords[0] != coords[-1]:
                coords.append(coords[0])  # Close polygon
            geometry = {"type": "Polygon", "coordinates": [coords]}
            # Calculate centroid
            lats = [pt["lat"] for pt in el["geometry"]]
            lngs = [pt["lon"] for pt in el["geometry"]]
            centroid_lat = sum(lats) / len(lats)
            centroid_lng = sum(lngs) / len(lngs)
        elif el_type == "relation" and "bounds" in el:
            # Relation - use bounds center as approximation
            bounds = el["bounds"]
            centroid_lat = (bounds["minlat"] + bounds["maxlat"]) / 2
            centroid_lng = (bounds["minlon"] + bounds["maxlon"]) / 2
            geometry = {"type": "Point", "coordinates": [centroid_lng, centroid_lat]}
        elif "center" in el:
            center = el["center"]
            centroid_lat, centroid_lng = center["lat"], center["lon"]
            geometry = {"type": "Point", "coordinates": [centroid_lng, centroid_lat]}

        if centroid_lat and centroid_lng and osm_id:
            distance = haversine_distance(lat, lng, centroid_lat, centroid_lng)
            leisure_type = el.get("tags", {}).get("leisure", "park")
            parks.append({
                "osm_id": osm_id,
                "name": el.get("tags", {}).get("name", f"Unnamed {leisure_type.title()}"),
                "type": leisure_type,  # park, garden, playground
                "distance_m": int(distance),
                "geometry": geometry,
                "centroid_lat": centroid_lat,
                "centroid_lng": centroid_lng,
            })

    return sorted(parks, key=lambda x: x["distance_m"])
```

**Step 2: Update coffee shop query similarly**

```python
async def get_nearby_coffee_shops(
    self, lat: float, lng: float, radius_m: int = 1000
) -> list[dict]:
    """Get coffee shops within radius."""
    query = f"""
    [out:json][timeout:5];
    (
      node["amenity"="cafe"](around:{radius_m},{lat},{lng});
      way["amenity"="cafe"](around:{radius_m},{lat},{lng});
      node["cuisine"="coffee"](around:{radius_m},{lat},{lng});
    );
    out center;
    """

    elements = await self._query_overpass(query)
    cafes = []

    for el in elements:
        osm_id = el.get("id")
        center = el.get("center", {})
        el_lat = center.get("lat") or el.get("lat")
        el_lng = center.get("lon") or el.get("lon")

        if el_lat and el_lng and osm_id:
            distance = haversine_distance(lat, lng, el_lat, el_lng)
            cafes.append({
                "osm_id": osm_id,
                "name": el.get("tags", {}).get("name", "Unnamed Cafe"),
                "type": "coffee_shop",
                "distance_m": int(distance),
                "geometry": {"type": "Point", "coordinates": [el_lng, el_lat]},
                "centroid_lat": el_lat,
                "centroid_lng": el_lng,
            })

    return sorted(cafes, key=lambda x: x["distance_m"])
```

**Step 3: Update dog park query similarly**

```python
async def get_nearby_dog_parks(
    self, lat: float, lng: float, radius_m: int = 2000
) -> list[dict]:
    """Get dog parks within radius."""
    query = f"""
    [out:json][timeout:5];
    (
      node["leisure"="dog_park"](around:{radius_m},{lat},{lng});
      way["leisure"="dog_park"](around:{radius_m},{lat},{lng});
    );
    out body geom;
    """

    elements = await self._query_overpass(query)
    dog_parks = []

    for el in elements:
        osm_id = el.get("id")
        el_type = el.get("type")

        geometry = None
        centroid_lat, centroid_lng = None, None

        if el_type == "way" and "geometry" in el:
            coords = [[pt["lon"], pt["lat"]] for pt in el["geometry"]]
            if coords and coords[0] != coords[-1]:
                coords.append(coords[0])
            geometry = {"type": "Polygon", "coordinates": [coords]}
            lats = [pt["lat"] for pt in el["geometry"]]
            lngs = [pt["lon"] for pt in el["geometry"]]
            centroid_lat = sum(lats) / len(lats)
            centroid_lng = sum(lngs) / len(lngs)
        elif el_type == "node":
            centroid_lat, centroid_lng = el.get("lat"), el.get("lon")
            geometry = {"type": "Point", "coordinates": [centroid_lng, centroid_lat]}
        elif "center" in el:
            center = el["center"]
            centroid_lat, centroid_lng = center["lat"], center["lon"]
            geometry = {"type": "Point", "coordinates": [centroid_lng, centroid_lat]}

        if centroid_lat and centroid_lng and osm_id:
            distance = haversine_distance(lat, lng, centroid_lat, centroid_lng)
            dog_parks.append({
                "osm_id": osm_id,
                "name": el.get("tags", {}).get("name", "Unnamed Dog Park"),
                "type": "dog_park",
                "distance_m": int(distance),
                "geometry": geometry,
                "centroid_lat": centroid_lat,
                "centroid_lng": centroid_lng,
            })

    return sorted(dog_parks, key=lambda x: x["distance_m"])
```

**Step 4: Run enricher tests**

Run: `cd /home/dawid/projects/domowik/backend && uv run pytest tests/scraper/test_enricher.py -v`
Expected: Tests pass (may need fixture updates)

**Step 5: Commit**

```bash
git add backend/scraper/enricher.py
git commit -m "feat(enricher): fetch full geometry for POIs"
```

---

## Task 4: Create POI Upsert Service

**Files:**
- Create: `backend/app/services/poi_service.py`

**Step 1: Write the POI service**

```python
# backend/app/services/poi_service.py
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
```

**Step 2: Verify syntax**

Run: `cd /home/dawid/projects/domowik/backend && uv run python -c "from app.services.poi_service import upsert_pois_for_listing; print('OK')"`
Expected: `OK`

**Step 3: Commit**

```bash
git add backend/app/services/poi_service.py
git commit -m "feat(services): add POI upsert service"
```

---

## Task 5: Update Enrich Script to Use POI Service

**Files:**
- Modify: `backend/scraper/enrich.py`

**Step 1: Read current enrich.py**

Read the file first to understand current structure.

**Step 2: Update to use POI service**

After enriching a listing, call `upsert_pois_for_listing` with the combined POI list:

```python
from app.services.poi_service import upsert_pois_for_listing

# In the enrichment loop, after getting amenity_data:
all_pois = amenity_data.parks + amenity_data.coffee_shops + amenity_data.dog_parks
await upsert_pois_for_listing(db, listing.id, all_pois)
```

**Step 3: Run enrich on a test listing**

Run: `cd /home/dawid/projects/domowik && task scraper:enrich -- --limit 1`
Expected: Completes without errors, POIs inserted

**Step 4: Verify POIs in database**

Run: `docker exec -it domowik-db-1 psql -U postgres -d housesearch -c "SELECT id, osm_id, type, name FROM points_of_interest LIMIT 5;"`
Expected: Shows POI records

**Step 5: Commit**

```bash
git add backend/scraper/enrich.py
git commit -m "feat(enrich): store POIs in dedicated table"
```

---

## Task 6: Add POI API Endpoint

**Files:**
- Create: `backend/app/api/pois.py`
- Modify: `backend/app/api/__init__.py` (or main router file)

**Step 1: Create POI schema**

```python
# backend/app/schemas/poi.py
from pydantic import BaseModel


class POIResponse(BaseModel):
    id: int
    osm_id: int
    type: str
    name: str | None
    geometry: dict  # GeoJSON

    class Config:
        from_attributes = True
```

**Step 2: Create POI endpoint**

```python
# backend/app/api/pois.py
from typing import Annotated
from fastapi import APIRouter, Depends, Query
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession
from geoalchemy2.functions import ST_AsGeoJSON

from app.models import get_db, User
from app.models.poi import PointOfInterest
from app.api.deps import get_current_user
from app.schemas.poi import POIResponse
import json

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
```

**Step 3: Register router**

Add to main app or router file:

```python
from app.api.pois import router as pois_router
app.include_router(pois_router, prefix="/api")
```

**Step 4: Test endpoint**

Run: `curl -H "Authorization: Bearer <token>" "http://localhost:8000/api/pois?ids=1&ids=2"`
Expected: Returns POI objects with geometry

**Step 5: Commit**

```bash
git add backend/app/schemas/poi.py backend/app/api/pois.py backend/app/main.py
git commit -m "feat(api): add GET /api/pois endpoint"
```

---

## Task 7: Add POI IDs to Listing Response

**Files:**
- Modify: `backend/app/schemas/listing.py`
- Modify: `backend/app/api/listings.py`
- Modify: `backend/app/models/listing.py`

**Step 1: Add relationship to Listing model**

In `backend/app/models/listing.py`, add:

```python
from app.models.poi import ListingPOI

class Listing(Base):
    # ... existing fields ...
    poi_links: Mapped[list["ListingPOI"]] = relationship(
        "ListingPOI",
        primaryjoin="Listing.id == ListingPOI.listing_id",
        foreign_keys="ListingPOI.listing_id",
    )
```

**Step 2: Update ListingResponse schema**

In `backend/app/schemas/listing.py`:

```python
class ListingResponse(BaseModel):
    # ... existing fields ...
    poi_ids: list[int] = []
```

**Step 3: Update listing_to_response function**

In `backend/app/api/listings.py`, update to include POI IDs:

```python
def listing_to_response(
    listing: Listing,
    lng: float | None,
    lat: float | None,
    user_status: UserListingStatus | None,
    last_visit: datetime | None,
) -> ListingResponse:
    # ... existing code ...

    poi_ids = [link.poi_id for link in listing.poi_links] if listing.poi_links else []

    return ListingResponse(
        # ... existing fields ...
        poi_ids=poi_ids,
    )
```

**Step 4: Add selectinload for poi_links**

Update queries to include:

```python
.options(selectinload(Listing.amenity_score), selectinload(Listing.poi_links))
```

**Step 5: Test listing endpoint**

Run: `curl -H "Authorization: Bearer <token>" "http://localhost:8000/api/listings/1"`
Expected: Response includes `poi_ids` array

**Step 6: Commit**

```bash
git add backend/app/models/listing.py backend/app/schemas/listing.py backend/app/api/listings.py
git commit -m "feat(api): include poi_ids in listing responses"
```

---

## Task 8: Add Frontend POI Types

**Files:**
- Modify: `frontend/src/types/index.ts`

**Step 1: Add POI type**

```typescript
export interface POI {
  id: number;
  osm_id: number;
  type: 'coffee_shop' | 'dog_park' | 'park' | 'garden' | 'playground';
  name: string | null;
  geometry: GeoJSON.Point | GeoJSON.Polygon;
}
```

**Step 2: Update Listing interface**

```typescript
export interface Listing {
  // ... existing fields ...
  poi_ids: number[];
}
```

**Step 3: Update AmenityScore interface**

Remove the embedded parks/coffee_shops arrays since we'll use POIs now.

**Step 4: Commit**

```bash
git add frontend/src/types/index.ts
git commit -m "feat(frontend): add POI types"
```

---

## Task 9: Add Frontend POI API Client

**Files:**
- Modify: `frontend/src/api/client.ts`

**Step 1: Add getPOIs method**

```typescript
// In ApiClient class
async getPOIs(ids: number[]): Promise<POI[]> {
  if (ids.length === 0) return [];

  const params = new URLSearchParams();
  ids.forEach(id => params.append('ids', String(id)));

  return this.request<POI[]>(`/api/pois?${params.toString()}`);
}
```

**Step 2: Import POI type**

```typescript
import type { Listing, Note, Preferences, ListingFilters, ClusterResponse, POI } from '../types';
```

**Step 3: Commit**

```bash
git add frontend/src/api/client.ts
git commit -m "feat(frontend): add getPOIs API method"
```

---

## Task 10: Create POI Cache Hook

**Files:**
- Create: `frontend/src/hooks/usePOICache.ts`

**Step 1: Write the cache hook**

```typescript
// frontend/src/hooks/usePOICache.ts
import { useRef, useCallback } from 'react';
import type { POI } from '../types';
import { api } from '../api/client';

interface CacheEntry {
  poi: POI;
  lastAccessed: number;
}

const MAX_CACHE_SIZE = 500;

export function usePOICache() {
  const cache = useRef<Map<number, CacheEntry>>(new Map());

  const get = useCallback((id: number): POI | undefined => {
    const entry = cache.current.get(id);
    if (entry) {
      entry.lastAccessed = Date.now();
      return entry.poi;
    }
    return undefined;
  }, []);

  const set = useCallback((poi: POI): void => {
    if (cache.current.size >= MAX_CACHE_SIZE) {
      // Evict LRU entry
      let oldestId: number | null = null;
      let oldestTime = Infinity;
      for (const [id, entry] of cache.current) {
        if (entry.lastAccessed < oldestTime) {
          oldestTime = entry.lastAccessed;
          oldestId = id;
        }
      }
      if (oldestId !== null) {
        cache.current.delete(oldestId);
      }
    }
    cache.current.set(poi.id, { poi, lastAccessed: Date.now() });
  }, []);

  const fetchPOIs = useCallback(async (ids: number[]): Promise<POI[]> => {
    const cached: POI[] = [];
    const missing: number[] = [];

    for (const id of ids) {
      const poi = get(id);
      if (poi) {
        cached.push(poi);
      } else {
        missing.push(id);
      }
    }

    if (missing.length > 0) {
      const fetched = await api.getPOIs(missing);
      for (const poi of fetched) {
        set(poi);
      }
      return [...cached, ...fetched];
    }

    return cached;
  }, [get, set]);

  return { get, fetchPOIs };
}
```

**Step 2: Commit**

```bash
git add frontend/src/hooks/usePOICache.ts
git commit -m "feat(frontend): add POI cache hook with LRU eviction"
```

---

## Task 11: Create POI Layer Component

**Files:**
- Create: `frontend/src/components/POILayer.tsx`

**Step 1: Write the POI layer component**

```typescript
// frontend/src/components/POILayer.tsx
import { useEffect, useState } from 'react';
import { CircleMarker, Polygon, Tooltip } from 'react-leaflet';
import type { POI } from '../types';
import { usePOICache } from '../hooks/usePOICache';

const SHOW_POI_ZOOM_THRESHOLD = 14;

// POI type colors
const POI_COLORS: Record<string, string> = {
  coffee_shop: '#8B4513',  // Brown
  dog_park: '#F97316',     // Orange
  park: '#22C55E',         // Green
  garden: '#16A34A',       // Darker green
  playground: '#A855F7',   // Purple
};

interface POILayerProps {
  poiIds: number[];
  zoom: number;
}

export function POILayer({ poiIds, zoom }: POILayerProps) {
  const { fetchPOIs } = usePOICache();
  const [pois, setPois] = useState<POI[]>([]);

  useEffect(() => {
    if (zoom < SHOW_POI_ZOOM_THRESHOLD || poiIds.length === 0) {
      setPois([]);
      return;
    }

    fetchPOIs(poiIds).then(setPois);
  }, [poiIds, zoom, fetchPOIs]);

  if (zoom < SHOW_POI_ZOOM_THRESHOLD) {
    return null;
  }

  return (
    <>
      {pois.map((poi) => {
        const color = POI_COLORS[poi.type] || '#6B7280';

        if (poi.geometry.type === 'Point') {
          const [lng, lat] = poi.geometry.coordinates;
          return (
            <CircleMarker
              key={poi.id}
              center={[lat, lng]}
              radius={6}
              pathOptions={{
                fillColor: color,
                fillOpacity: 0.8,
                color: '#FFFFFF',
                weight: 2,
              }}
            >
              <Tooltip>
                <div className="text-sm">
                  <div className="font-semibold">{poi.name || poi.type}</div>
                  <div className="text-gray-500 capitalize">{poi.type.replace('_', ' ')}</div>
                </div>
              </Tooltip>
            </CircleMarker>
          );
        }

        if (poi.geometry.type === 'Polygon') {
          // Convert GeoJSON [lng, lat] to Leaflet [lat, lng]
          const positions = poi.geometry.coordinates[0].map(
            ([lng, lat]) => [lat, lng] as [number, number]
          );
          return (
            <Polygon
              key={poi.id}
              positions={positions}
              pathOptions={{
                color: color,
                weight: 2,
                fillColor: color,
                fillOpacity: 0.15,
              }}
            >
              <Tooltip>
                <div className="text-sm">
                  <div className="font-semibold">{poi.name || poi.type}</div>
                  <div className="text-gray-500 capitalize">{poi.type.replace('_', ' ')}</div>
                </div>
              </Tooltip>
            </Polygon>
          );
        }

        return null;
      })}
    </>
  );
}
```

**Step 2: Commit**

```bash
git add frontend/src/components/POILayer.tsx
git commit -m "feat(frontend): add POILayer component for map rendering"
```

---

## Task 12: Integrate POI Layer into Map

**Files:**
- Modify: `frontend/src/components/Map.tsx`
- Modify: `frontend/src/pages/Dashboard.tsx`

**Step 1: Import and add POILayer to Map**

In `Map.tsx`:

```typescript
import { POILayer } from './POILayer';

interface MapProps {
  // ... existing props ...
  selectedPoiIds?: number[];
  zoom?: number;
}

// In MapContainer:
{selectedPoiIds && selectedPoiIds.length > 0 && (
  <POILayer poiIds={selectedPoiIds} zoom={zoom ?? 11} />
)}
```

**Step 2: Track zoom level in Map component**

Add internal zoom state tracking:

```typescript
function ZoomTracker({ onZoomChange }: { onZoomChange: (zoom: number) => void }) {
  const map = useMap();

  useMapEvents({
    zoomend: () => {
      onZoomChange(map.getZoom());
    },
  });

  useEffect(() => {
    onZoomChange(map.getZoom());
  }, [map, onZoomChange]);

  return null;
}
```

**Step 3: Pass selected listing's POI IDs from Dashboard**

In `Dashboard.tsx`:

```typescript
<Map
  // ... existing props ...
  selectedPoiIds={selectedListing?.poi_ids}
/>
```

**Step 4: Test POI rendering**

1. Start frontend: `cd frontend && npm run dev`
2. Select a listing at zoom >= 14
3. Verify POI markers/polygons appear

**Step 5: Commit**

```bash
git add frontend/src/components/Map.tsx frontend/src/pages/Dashboard.tsx
git commit -m "feat(frontend): integrate POI layer into map"
```

---

## Task 13: Migration Script for Existing Data

**Files:**
- Create: `backend/scripts/migrate_existing_pois.py`

**Step 1: Write migration script**

```python
#!/usr/bin/env python
"""
Migrate existing POI data from amenity_scores JSONB to points_of_interest table.
Run this once after deploying the new schema.
"""
import asyncio
import json
from sqlalchemy import select, text
from sqlalchemy.ext.asyncio import create_async_engine, AsyncSession
from sqlalchemy.orm import sessionmaker

from app.config import settings
from app.models.amenity import AmenityScore
from app.models.poi import PointOfInterest, ListingPOI


async def migrate():
    engine = create_async_engine(settings.database_url)
    async_session = sessionmaker(engine, class_=AsyncSession, expire_on_commit=False)

    async with async_session() as db:
        # Get all amenity scores with POI data
        result = await db.execute(
            select(AmenityScore).where(
                (AmenityScore.parks.is_not(None)) |
                (AmenityScore.coffee_shops.is_not(None))
            )
        )
        scores = result.scalars().all()

        print(f"Migrating POIs from {len(scores)} listings...")

        for score in scores:
            pois = []

            # Parks (stored as dicts with lat/lng but no osm_id - skip these)
            # We'll need to re-enrich to get proper OSM IDs

            # For now, just log what needs re-enrichment
            if score.parks:
                print(f"Listing {score.listing_id} has {len(score.parks)} parks - needs re-enrichment")
            if score.coffee_shops:
                print(f"Listing {score.listing_id} has {len(score.coffee_shops)} coffee shops - needs re-enrichment")

        print("Migration complete. Run enrichment to populate POI table with OSM data.")


if __name__ == "__main__":
    asyncio.run(migrate())
```

**Step 2: Commit**

```bash
git add backend/scripts/migrate_existing_pois.py
git commit -m "feat(scripts): add POI migration script"
```

---

## Task 14: Update Tests

**Files:**
- Modify: `backend/tests/conftest.py`
- Create: `backend/tests/test_pois.py`

**Step 1: Add POI fixtures to conftest.py**

```python
@pytest.fixture
def overpass_parks_response_with_geometry() -> dict:
    """Mock Overpass API response for parks with geometry."""
    return {
        "elements": [
            {
                "type": "way",
                "id": 123456,
                "tags": {"name": "Stanley Park", "leisure": "park"},
                "geometry": [
                    {"lat": 49.2830, "lon": -123.1200},
                    {"lat": 49.2835, "lon": -123.1190},
                    {"lat": 49.2825, "lon": -123.1185},
                    {"lat": 49.2820, "lon": -123.1195},
                ],
            },
        ]
    }
```

**Step 2: Write POI API test**

```python
# backend/tests/test_pois.py
import pytest
from httpx import AsyncClient


@pytest.mark.asyncio
async def test_get_pois_empty(client: AsyncClient, auth_headers: dict):
    response = await client.get("/api/pois?ids=999999", headers=auth_headers)
    assert response.status_code == 200
    assert response.json() == []
```

**Step 3: Run tests**

Run: `cd /home/dawid/projects/domowik/backend && uv run pytest tests/ -v`
Expected: All tests pass

**Step 4: Commit**

```bash
git add backend/tests/conftest.py backend/tests/test_pois.py
git commit -m "test: add POI fixtures and tests"
```

---

## Summary

This plan covers:
1. **Backend models** for POI storage (Task 1-2)
2. **Enricher updates** to fetch geometry (Task 3)
3. **POI service** for upsert logic (Task 4-5)
4. **API endpoints** for fetching POIs (Task 6-7)
5. **Frontend types and API** (Task 8-9)
6. **POI cache hook** with LRU eviction (Task 10)
7. **Map POI rendering** (Task 11-12)
8. **Migration and tests** (Task 13-14)

After implementation, run full enrichment to populate the POI table:
```bash
task scraper:enrich
```

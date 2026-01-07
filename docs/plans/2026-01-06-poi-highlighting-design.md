# POI Highlighting on Listing Selection

## Overview

When a user selects a listing (click on map marker or sidebar), display the nearby points of interest (POIs) that contribute to that listing's amenity score. POIs are stored as first-class entities and shared across listings.

## POI Types

| Type | Geometry | Display |
|------|----------|---------|
| coffee_shop | Point | Dot marker |
| dog_park | Point | Dot marker |
| metro_stop | Point | Dot marker |
| park | Polygon | Contour outline |
| playground | Polygon | Contour outline |

## Data Model

### PointOfInterest Table (new)

```sql
CREATE TABLE points_of_interest (
    id SERIAL PRIMARY KEY,
    osm_id BIGINT UNIQUE NOT NULL,
    type VARCHAR(50) NOT NULL,  -- 'coffee_shop', 'dog_park', 'park', etc.
    name VARCHAR(255),
    geometry GEOMETRY NOT NULL,  -- Point or Polygon (PostGIS)
    centroid GEOMETRY(Point, 4326) GENERATED ALWAYS AS (ST_Centroid(geometry)) STORED,
    created_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_poi_osm_id ON points_of_interest(osm_id);
CREATE INDEX idx_poi_type ON points_of_interest(type);
CREATE INDEX idx_poi_geometry ON points_of_interest USING GIST(geometry);
CREATE INDEX idx_poi_centroid ON points_of_interest USING GIST(centroid);
```

### ListingPOI Junction Table (new)

```sql
CREATE TABLE listing_pois (
    listing_id INTEGER REFERENCES listings(id) ON DELETE CASCADE,
    poi_id INTEGER REFERENCES points_of_interest(id) ON DELETE CASCADE,
    distance_m INTEGER NOT NULL,  -- precomputed distance from listing to POI centroid
    PRIMARY KEY (listing_id, poi_id)
);

CREATE INDEX idx_listing_pois_listing ON listing_pois(listing_id);
CREATE INDEX idx_listing_pois_poi ON listing_pois(poi_id);
```

### AmenityScore Table Changes

Remove duplicated POI data, keep only computed scores:

```python
class AmenityScore(Base):
    listing_id: int  # FK, PK
    nearest_park_m: int | None
    nearest_coffee_m: int | None
    nearest_dog_park_m: int | None
    walkability_score: int | None
    amenity_score: int | None
    # REMOVE: parks, coffee_shops JSONB columns
```

## API Changes

### Listing Response

Add POI IDs to listing response:

```json
{
  "id": 123,
  "address": "...",
  "amenity_score": { ... },
  "poi_ids": [1, 5, 12, 34, 56]  // NEW: IDs of linked POIs
}
```

### New Endpoint: GET /api/pois

Fetch POI details by IDs:

```
GET /api/pois?ids=1,5,12,34,56
```

Response:

```json
[
  {
    "id": 1,
    "osm_id": 123456789,
    "type": "coffee_shop",
    "name": "Blue Bottle Coffee",
    "geometry": {
      "type": "Point",
      "coordinates": [-123.1207, 49.2827]
    }
  },
  {
    "id": 12,
    "type": "park",
    "name": "Stanley Park",
    "geometry": {
      "type": "Polygon",
      "coordinates": [[[...], [...], ...]]
    }
  }
]
```

## Enrichment Flow

When enriching a listing:

1. Query Overpass API for POIs within 2km radius of listing coordinates
2. For each POI returned:
   - Check if `osm_id` exists in `points_of_interest`
   - If not: insert with full geometry (point or polygon from OSM)
   - If yes: skip (already in DB)
3. Create `listing_pois` links with precomputed distances
4. Calculate `nearest_*_m` values from linked POIs
5. Compute walkability/amenity scores as before

### Overpass Query Changes

Current: Only fetches center points
New: Fetch full geometry for areas (parks, playgrounds)

```overpass
[out:json];
(
  // Point amenities - center is fine
  node["amenity"="cafe"](around:2000, {lat}, {lng});
  node["leisure"="dog_park"](around:2000, {lat}, {lng});

  // Area amenities - need geometry
  way["leisure"="park"](around:2000, {lat}, {lng});
  relation["leisure"="park"](around:2000, {lat}, {lng});
  way["leisure"="playground"](around:2000, {lat}, {lng});
);
out body geom;
```

## Frontend Implementation

### POI Cache

```typescript
class POICache {
  private cache: Map<number, { poi: POI; lastAccessed: number }>;
  private maxSize: number = 500;  // configurable

  get(id: number): POI | undefined {
    const entry = this.cache.get(id);
    if (entry) {
      entry.lastAccessed = Date.now();
      return entry.poi;
    }
    return undefined;
  }

  set(id: number, poi: POI): void {
    if (this.cache.size >= this.maxSize) {
      this.evictLRU();
    }
    this.cache.set(id, { poi, lastAccessed: Date.now() });
  }

  private evictLRU(): void {
    let oldest: number | null = null;
    let oldestTime = Infinity;
    for (const [id, entry] of this.cache) {
      if (entry.lastAccessed < oldestTime) {
        oldestTime = entry.lastAccessed;
        oldest = id;
      }
    }
    if (oldest !== null) {
      this.cache.delete(oldest);
    }
  }
}
```

### Zoom Level Gating

Only show POI markers when zoom >= 14 (same threshold as individual listings vs clusters).

```typescript
const SHOW_POI_ZOOM_THRESHOLD = 14;

// In Map component
const shouldShowPOIs = zoom >= SHOW_POI_ZOOM_THRESHOLD && selectedListing;
```

### Selection Flow

1. User clicks listing → `selectedListing` set in state
2. Check zoom level >= 14, if not, skip POI display
3. Get `poi_ids` from selected listing
4. Check cache for each ID, collect missing IDs
5. If missing IDs: `GET /api/pois?ids=...` → add to cache
6. Render POIs on map:
   - Points (coffee_shop, dog_park): `CircleMarker` or custom icon
   - Polygons (park, playground): `Polygon` component with stroke, no fill (or light fill)

### POI Layer Component

```typescript
interface POILayerProps {
  poiIds: number[];
  zoom: number;
}

function POILayer({ poiIds, zoom }: POILayerProps) {
  const { cache, fetchPOIs } = usePOICache();
  const [pois, setPois] = useState<POI[]>([]);

  useEffect(() => {
    if (zoom < SHOW_POI_ZOOM_THRESHOLD) {
      setPois([]);
      return;
    }

    const cached: POI[] = [];
    const missing: number[] = [];

    for (const id of poiIds) {
      const poi = cache.get(id);
      if (poi) cached.push(poi);
      else missing.push(id);
    }

    if (missing.length > 0) {
      fetchPOIs(missing).then(fetched => {
        setPois([...cached, ...fetched]);
      });
    } else {
      setPois(cached);
    }
  }, [poiIds, zoom]);

  return (
    <>
      {pois.map(poi => (
        poi.geometry.type === 'Point'
          ? <POIMarker key={poi.id} poi={poi} />
          : <POIPolygon key={poi.id} poi={poi} />
      ))}
    </>
  );
}
```

### Visual Styling

| POI Type | Color | Style |
|----------|-------|-------|
| coffee_shop | Brown (#8B4513) | Filled circle, 8px |
| dog_park | Orange (#F97316) | Filled circle, 8px |
| park | Green (#22C55E) | Polygon outline, 2px stroke, 10% fill |
| playground | Purple (#A855F7) | Polygon outline, 2px stroke, 10% fill |
| metro_stop | Blue (#3B82F6) | Filled circle, 8px |

## Migration Strategy

1. Create new tables (`points_of_interest`, `listing_pois`)
2. Run migration script to:
   - Extract unique POIs from existing `amenity_scores.parks` and `amenity_scores.coffee_shops`
   - Insert into `points_of_interest` (without full geometry initially - just centroids as points)
   - Create `listing_pois` links
3. Update enricher to use new flow
4. Re-enrich listings to get full polygon geometries for parks
5. Remove deprecated JSONB columns from `amenity_scores`

## Performance Considerations

- POI table expected size: ~10k-50k records (Vancouver area)
- Listing-POI links: ~50k-500k records (avg 5-50 POIs per listing within 2km)
- Frontend cache: 500 POIs max (~50KB-500KB memory depending on polygon complexity)
- Batch POI fetches: Request up to 100 IDs per call to reduce round trips

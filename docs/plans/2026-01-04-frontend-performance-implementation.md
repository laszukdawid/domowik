# Frontend Performance Optimization Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Optimize frontend to render 10k listings instantly using viewport-based filtering, marker clustering, and aggregated sidebar.

**Architecture:** Backend filters by bounding box and clusters listings using DBSCAN. Frontend shows clustered markers on map and aggregated area cards in sidebar, expanding to virtualized list on drill-down.

**Tech Stack:** React, Leaflet, react-leaflet-cluster, react-window, scikit-learn (DBSCAN), GeoAlchemy2

---

## Task 1: Add Backend Bounding Box Filter

**Files:**
- Modify: `backend/app/api/listings.py:52-104`
- Modify: `backend/app/api/listings.py:107-157`

**Step 1: Add bbox parameter parsing to build_listings_query**

Add bbox filtering to the existing `build_listings_query` function in `backend/app/api/listings.py`:

```python
from geoalchemy2.functions import ST_X, ST_Y, ST_MakeEnvelope, ST_Within
```

Add to `build_listings_query` signature:
```python
def build_listings_query(
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
    bbox: str | None = None,  # "minLng,minLat,maxLng,maxLat"
):
```

Add bbox filter before the ORDER BY:
```python
    if bbox:
        try:
            min_lng, min_lat, max_lng, max_lat = [float(x) for x in bbox.split(",")]
            envelope = ST_MakeEnvelope(min_lng, min_lat, max_lng, max_lat, 4326)
            query = query.where(ST_Within(Listing.location, envelope))
        except (ValueError, AttributeError):
            pass  # Invalid bbox, skip filtering
```

**Step 2: Add bbox to stream_listings endpoint**

Update `stream_listings` function signature:
```python
@router.get("/stream")
async def stream_listings(
    db: Annotated[AsyncSession, Depends(get_db)],
    user: Annotated[User, Depends(get_current_user)],
    min_price: int | None = None,
    max_price: int | None = None,
    min_bedrooms: int | None = None,
    min_sqft: int | None = None,
    cities: list[str] | None = Query(None),
    property_types: list[str] | None = Query(None),
    include_hidden: bool = False,
    favorites_only: bool = False,
    min_score: int | None = None,
    chunk_size: int = 25,
    bbox: str | None = None,  # ADD THIS
):
```

Pass bbox to query builder:
```python
        query = build_listings_query(
            user, min_price, max_price, min_bedrooms, min_sqft,
            cities, property_types, include_hidden, favorites_only, min_score,
            bbox  # ADD THIS
        )
```

**Step 3: Test manually**

Run: `curl "http://localhost:8000/api/listings/stream?bbox=-123.2,-49.2,-123.0,49.3" -H "Authorization: Bearer <token>"`

Expected: Only listings within the bounding box are returned.

**Step 4: Commit**

```bash
git add backend/app/api/listings.py
git commit -m "feat(api): add bounding box filter to listings endpoint"
```

---

## Task 2: Add Backend Clustering Endpoint

**Files:**
- Create: `backend/app/api/clusters.py`
- Modify: `backend/app/main.py` (add router)

**Step 1: Create the clusters module**

Create `backend/app/api/clusters.py`:

```python
from typing import Annotated
from datetime import datetime, timedelta, UTC
from fastapi import APIRouter, Depends, Query
from sqlalchemy import select, and_, func
from sqlalchemy.orm import selectinload
from sqlalchemy.ext.asyncio import AsyncSession
from geoalchemy2.functions import ST_X, ST_Y, ST_MakeEnvelope, ST_Within
from sklearn.cluster import DBSCAN
import numpy as np

from app.models import get_db, Listing, AmenityScore, UserListingStatus, User
from app.api.deps import get_current_user
from app.api.listings import build_listings_query

router = APIRouter(prefix="/clusters", tags=["clusters"])


@router.get("")
async def get_clusters(
    db: Annotated[AsyncSession, Depends(get_db)],
    user: Annotated[User, Depends(get_current_user)],
    bbox: str,  # Required: "minLng,minLat,maxLng,maxLat"
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

    # Parse bbox
    try:
        min_lng, min_lat, max_lng, max_lat = [float(x) for x in bbox.split(",")]
    except ValueError:
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
```

**Step 2: Register the router**

In `backend/app/main.py`, add:

```python
from app.api.clusters import router as clusters_router

# After other router includes:
app.include_router(clusters_router, prefix="/api")
```

**Step 3: Add scikit-learn dependency**

Run: `cd backend && uv add scikit-learn`

**Step 4: Test manually**

Run: `curl "http://localhost:8000/api/clusters?bbox=-123.3,49.1,-122.9,49.4&zoom=11" -H "Authorization: Bearer <token>"`

Expected: JSON with `clusters` array and `outliers` array.

**Step 5: Commit**

```bash
git add backend/app/api/clusters.py backend/app/main.py pyproject.toml uv.lock
git commit -m "feat(api): add clustering endpoint with DBSCAN"
```

---

## Task 3: Add Frontend Types for Clusters

**Files:**
- Modify: `frontend/src/types/index.ts`

**Step 1: Add cluster types**

Add to `frontend/src/types/index.ts`:

```typescript
export interface ClusterStats {
  price_min: number;
  price_max: number;
  price_avg: number;
  beds_min: number | null;
  beds_max: number | null;
  amenity_avg: number | null;
}

export interface Cluster {
  id: string;
  label: string;
  center: { lat: number; lng: number };
  bounds: { north: number; south: number; east: number; west: number };
  count: number;
  stats: ClusterStats;
  listing_ids: number[];
}

export interface ClusterOutlier {
  id: number;
  lat: number;
  lng: number;
  price: number;
  bedrooms: number | null;
  address: string;
  amenity_score: number | null;
  is_favorite: boolean;
}

export interface ClusterResponse {
  clusters: Cluster[];
  outliers: ClusterOutlier[];
}

export interface BBox {
  minLng: number;
  minLat: number;
  maxLng: number;
  maxLat: number;
}
```

**Step 2: Extend ListingFilters**

Add to the existing `ListingFilters` interface:

```typescript
export interface ListingFilters {
  min_price?: number;
  max_price?: number;
  min_bedrooms?: number;
  min_sqft?: number;
  cities?: string[];
  property_types?: string[];
  include_hidden?: boolean;
  favorites_only?: boolean;
  min_score?: number;
  bbox?: string;  // ADD THIS
}
```

**Step 3: Commit**

```bash
git add frontend/src/types/index.ts
git commit -m "feat(types): add cluster and bbox types"
```

---

## Task 4: Add API Client Methods for Clusters

**Files:**
- Modify: `frontend/src/api/client.ts`

**Step 1: Add getClusters method**

Add to the `ApiClient` class in `frontend/src/api/client.ts`:

```typescript
  async getClusters(
    bbox: string,
    zoom: number,
    filters: ListingFilters = {}
  ): Promise<ClusterResponse> {
    const params = new URLSearchParams();
    params.append('bbox', bbox);
    params.append('zoom', String(zoom));

    Object.entries(filters).forEach(([key, value]) => {
      if (value !== undefined && value !== null && key !== 'bbox') {
        if (Array.isArray(value)) {
          value.forEach((v) => params.append(key, String(v)));
        } else {
          params.append(key, String(value));
        }
      }
    });

    return this.request<ClusterResponse>(`/api/clusters?${params.toString()}`);
  }
```

**Step 2: Add import**

Update imports at top of file:

```typescript
import type { Listing, Note, Preferences, ListingFilters, ClusterResponse } from '../types';
```

**Step 3: Commit**

```bash
git add frontend/src/api/client.ts
git commit -m "feat(api): add getClusters client method"
```

---

## Task 5: Create useClusters Hook

**Files:**
- Create: `frontend/src/hooks/useClusters.ts`

**Step 1: Create the hook**

Create `frontend/src/hooks/useClusters.ts`:

```typescript
import { useQuery } from '@tanstack/react-query';
import { api } from '../api/client';
import type { ListingFilters, BBox, ClusterResponse } from '../types';

interface UseClusterParams {
  bbox: BBox | null;
  zoom: number;
  filters: ListingFilters;
  enabled?: boolean;
}

export function useClusters({ bbox, zoom, filters, enabled = true }: UseClusterParams) {
  const bboxString = bbox
    ? `${bbox.minLng},${bbox.minLat},${bbox.maxLng},${bbox.maxLat}`
    : null;

  return useQuery<ClusterResponse>({
    queryKey: ['clusters', bboxString, zoom, filters],
    queryFn: () => api.getClusters(bboxString!, zoom, filters),
    enabled: enabled && !!bboxString,
    staleTime: 30000, // 30 seconds - clusters don't change often
    placeholderData: (prev) => prev, // Keep previous while fetching
  });
}
```

**Step 2: Commit**

```bash
git add frontend/src/hooks/useClusters.ts
git commit -m "feat(hooks): add useClusters hook"
```

---

## Task 6: Add Map Bounds Tracking

**Files:**
- Modify: `frontend/src/components/Map.tsx`

**Step 1: Create MapBoundsTracker component**

Add a new component inside `Map.tsx` that tracks bounds:

```typescript
import { useEffect, useRef } from 'react';
import { useMap, useMapEvents } from 'react-leaflet';
import type { BBox } from '../types';

interface MapBoundsTrackerProps {
  onBoundsChange: (bbox: BBox, zoom: number) => void;
  debounceMs?: number;
}

function MapBoundsTracker({ onBoundsChange, debounceMs = 300 }: MapBoundsTrackerProps) {
  const map = useMap();
  const timeoutRef = useRef<number | null>(null);

  const emitBounds = () => {
    const bounds = map.getBounds();
    const zoom = map.getZoom();

    onBoundsChange({
      minLng: bounds.getWest(),
      minLat: bounds.getSouth(),
      maxLng: bounds.getEast(),
      maxLat: bounds.getNorth(),
    }, zoom);
  };

  useMapEvents({
    moveend: () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
      timeoutRef.current = window.setTimeout(emitBounds, debounceMs);
    },
    zoomend: () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
      timeoutRef.current = window.setTimeout(emitBounds, debounceMs);
    },
  });

  // Emit initial bounds on mount
  useEffect(() => {
    emitBounds();
    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
    };
  }, []);

  return null;
}
```

**Step 2: Update MapProps interface**

```typescript
interface MapProps {
  listings: Listing[];
  selectedId?: number;
  onSelect: (listing: Listing) => void;
  onBoundsChange?: (bbox: BBox, zoom: number) => void;  // ADD THIS
}
```

**Step 3: Add tracker to Map component**

In the `Map` component, add inside `MapContainer`:

```typescript
export default function Map({ listings, selectedId, onSelect, onBoundsChange }: MapProps) {
  // ... existing code ...

  return (
    <MapContainer
      center={[49.2827, -123.1207]}
      zoom={11}
      className="h-full w-full"
    >
      <TileLayer
        attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>'
        url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
      />
      <MapUpdater listings={validListings} />
      {onBoundsChange && <MapBoundsTracker onBoundsChange={onBoundsChange} />}
      {/* ... rest of markers ... */}
    </MapContainer>
  );
}
```

**Step 4: Add import**

At top of file:

```typescript
import { useEffect, useRef } from 'react';
import type { Listing, BBox } from '../types';
```

**Step 5: Commit**

```bash
git add frontend/src/components/Map.tsx
git commit -m "feat(map): add viewport bounds tracking with debounce"
```

---

## Task 7: Install Frontend Dependencies

**Files:**
- Modify: `frontend/package.json`

**Step 1: Install react-window for virtual scrolling**

Run:
```bash
cd frontend && npm install react-window @types/react-window
```

**Step 2: Install react-leaflet-cluster for marker clustering**

Run:
```bash
cd frontend && npm install react-leaflet-cluster
```

**Step 3: Commit**

```bash
git add frontend/package.json frontend/package-lock.json
git commit -m "feat(deps): add react-window and react-leaflet-cluster"
```

---

## Task 8: Create ClusterCard Component

**Files:**
- Create: `frontend/src/components/ClusterCard.tsx`

**Step 1: Create the component**

Create `frontend/src/components/ClusterCard.tsx`:

```typescript
import type { Cluster } from '../types';

interface ClusterCardProps {
  cluster: Cluster;
  onClick: () => void;
  isExpanded?: boolean;
}

export default function ClusterCard({ cluster, onClick, isExpanded }: ClusterCardProps) {
  const { stats } = cluster;

  const formatPrice = (price: number) => {
    if (price >= 1000000) {
      return `$${(price / 1000000).toFixed(1)}M`;
    }
    return `$${(price / 1000).toFixed(0)}K`;
  };

  const bedsRange = stats.beds_min && stats.beds_max
    ? stats.beds_min === stats.beds_max
      ? `${stats.beds_min} bd`
      : `${stats.beds_min}-${stats.beds_max} bd`
    : null;

  return (
    <div
      onClick={onClick}
      className={`
        p-3 rounded-lg border cursor-pointer transition-colors
        ${isExpanded
          ? 'border-blue-500 bg-blue-50'
          : 'border-gray-200 hover:border-gray-300 bg-white hover:bg-gray-50'
        }
      `}
    >
      <div className="flex items-center justify-between mb-1">
        <span className="font-medium text-gray-900">{cluster.label}</span>
        <span className="text-sm font-semibold text-blue-600">
          {cluster.count} listings
        </span>
      </div>

      <div className="flex items-center gap-3 text-sm text-gray-600">
        <span>{formatPrice(stats.price_min)} - {formatPrice(stats.price_max)}</span>
        {bedsRange && <span>{bedsRange}</span>}
        {stats.amenity_avg && (
          <span className="text-green-600">Score: {stats.amenity_avg}</span>
        )}
      </div>
    </div>
  );
}
```

**Step 2: Commit**

```bash
git add frontend/src/components/ClusterCard.tsx
git commit -m "feat(components): add ClusterCard for aggregated area display"
```

---

## Task 9: Update ListingSidebar with Aggregated Mode

**Files:**
- Modify: `frontend/src/components/ListingSidebar.tsx`

**Step 1: Rewrite ListingSidebar**

Replace the contents of `frontend/src/components/ListingSidebar.tsx`:

```typescript
import { useState, useCallback } from 'react';
import { FixedSizeList as List } from 'react-window';
import type { Listing, Cluster, ClusterOutlier } from '../types';
import ListingCard from './ListingCard';
import ClusterCard from './ClusterCard';

interface ListingSidebarProps {
  clusters: Cluster[];
  outliers: ClusterOutlier[];
  listings: Listing[];  // Full listing data for expanded view
  isLoading: boolean;
  totalCount: number;
  onSelect: (listing: Listing | ClusterOutlier) => void;
  onClusterClick: (cluster: Cluster) => void;
}

export default function ListingSidebar({
  clusters,
  outliers,
  listings,
  isLoading,
  totalCount,
  onSelect,
  onClusterClick,
}: ListingSidebarProps) {
  const [expandedClusterId, setExpandedClusterId] = useState<string | null>(null);

  const expandedCluster = expandedClusterId
    ? clusters.find(c => c.id === expandedClusterId)
    : null;

  const expandedListings = expandedCluster
    ? listings.filter(l => expandedCluster.listing_ids.includes(l.id))
    : [];

  const handleClusterClick = (cluster: Cluster) => {
    if (expandedClusterId === cluster.id) {
      setExpandedClusterId(null);
    } else {
      setExpandedClusterId(cluster.id);
      onClusterClick(cluster);
    }
  };

  const handleBack = () => {
    setExpandedClusterId(null);
  };

  // Virtualized row renderer for expanded view
  const Row = useCallback(({ index, style }: { index: number; style: React.CSSProperties }) => {
    const listing = expandedListings[index];
    return (
      <div style={style} className="px-4 py-1">
        <ListingCard
          listing={listing}
          onClick={() => onSelect(listing)}
          compact
        />
      </div>
    );
  }, [expandedListings, onSelect]);

  // Show expanded cluster view
  if (expandedCluster && expandedListings.length > 0) {
    return (
      <div className="h-full flex flex-col">
        <div className="p-4 border-b bg-gray-50">
          <button
            onClick={handleBack}
            className="flex items-center gap-2 text-sm text-gray-600 hover:text-gray-900 mb-2"
          >
            <span>←</span>
            <span>Back to areas</span>
          </button>
          <h2 className="font-semibold text-gray-900">
            {expandedCluster.label} ({expandedCluster.count})
          </h2>
        </div>

        <div className="flex-1">
          <List
            height={600}
            itemCount={expandedListings.length}
            itemSize={80}
            width="100%"
          >
            {Row}
          </List>
        </div>
      </div>
    );
  }

  // Show aggregated view
  return (
    <div className="p-4">
      <div className="mb-4">
        <h2 className="text-sm font-semibold text-gray-500 uppercase">
          {isLoading ? 'Loading...' : `Viewport: ${totalCount} listings`}
        </h2>
      </div>

      {clusters.length > 0 && (
        <div className="space-y-2 mb-4">
          {clusters.map((cluster) => (
            <ClusterCard
              key={cluster.id}
              cluster={cluster}
              onClick={() => handleClusterClick(cluster)}
              isExpanded={expandedClusterId === cluster.id}
            />
          ))}
        </div>
      )}

      {outliers.length > 0 && (
        <div>
          <h3 className="text-xs font-semibold text-gray-400 uppercase mb-2">
            Individual listings ({outliers.length})
          </h3>
          <div className="space-y-2">
            {outliers.slice(0, 10).map((outlier) => (
              <div
                key={outlier.id}
                onClick={() => onSelect(outlier)}
                className="p-2 rounded border border-gray-200 hover:border-gray-300 cursor-pointer bg-white hover:bg-gray-50"
              >
                <div className="text-sm font-medium truncate">{outlier.address}</div>
                <div className="text-sm text-gray-600">
                  ${outlier.price.toLocaleString()}
                  {outlier.bedrooms && ` · ${outlier.bedrooms} bd`}
                </div>
              </div>
            ))}
            {outliers.length > 10 && (
              <div className="text-sm text-gray-500 text-center">
                +{outliers.length - 10} more
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
```

**Step 2: Commit**

```bash
git add frontend/src/components/ListingSidebar.tsx
git commit -m "feat(sidebar): implement aggregated and virtualized modes"
```

---

## Task 10: Add Clustered Markers to Map

**Files:**
- Modify: `frontend/src/components/Map.tsx`

**Step 1: Add cluster marker support**

Update imports:

```typescript
import { MapContainer, TileLayer, Marker, Popup, useMap, useMapEvents, CircleMarker } from 'react-leaflet';
import MarkerClusterGroup from 'react-leaflet-cluster';
```

**Step 2: Add ClusterMarker component**

Add inside `Map.tsx`:

```typescript
interface ClusterMarkerProps {
  center: { lat: number; lng: number };
  count: number;
  label: string;
  onClick: () => void;
}

function ClusterMarker({ center, count, label, onClick }: ClusterMarkerProps) {
  // Size based on count
  const size = Math.min(60, Math.max(30, 20 + Math.log10(count) * 15));

  return (
    <CircleMarker
      center={[center.lat, center.lng]}
      radius={size / 2}
      pathOptions={{
        fillColor: '#3B82F6',
        fillOpacity: 0.8,
        color: '#1D4ED8',
        weight: 2,
      }}
      eventHandlers={{ click: onClick }}
    >
      <Popup>
        <div className="text-center">
          <div className="font-semibold">{label}</div>
          <div className="text-sm text-gray-600">{count} listings</div>
        </div>
      </Popup>
    </CircleMarker>
  );
}
```

**Step 3: Update MapProps and component**

```typescript
import type { Listing, BBox, Cluster, ClusterOutlier } from '../types';

interface MapProps {
  listings: Listing[];
  clusters?: Cluster[];
  outliers?: ClusterOutlier[];
  selectedId?: number;
  onSelect: (listing: Listing | ClusterOutlier) => void;
  onBoundsChange?: (bbox: BBox, zoom: number) => void;
  onClusterClick?: (cluster: Cluster) => void;
}

export default function Map({
  listings,
  clusters = [],
  outliers = [],
  selectedId,
  onSelect,
  onBoundsChange,
  onClusterClick,
}: MapProps) {
  const validListings = listings.filter((l) => l.latitude && l.longitude);

  return (
    <MapContainer
      center={[49.2827, -123.1207]}
      zoom={11}
      className="h-full w-full"
    >
      <TileLayer
        attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>'
        url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
      />
      <MapUpdater listings={validListings} />
      {onBoundsChange && <MapBoundsTracker onBoundsChange={onBoundsChange} />}

      {/* Cluster markers */}
      {clusters.map((cluster) => (
        <ClusterMarker
          key={cluster.id}
          center={cluster.center}
          count={cluster.count}
          label={cluster.label}
          onClick={() => onClusterClick?.(cluster)}
        />
      ))}

      {/* Outlier markers */}
      {outliers.map((outlier) => (
        <Marker
          key={`outlier-${outlier.id}`}
          position={[outlier.lat, outlier.lng]}
          icon={createOutlierIcon(outlier)}
          eventHandlers={{
            click: () => onSelect(outlier as any),
          }}
        >
          <Popup>
            <div className="text-sm">
              <div className="font-semibold">{outlier.address}</div>
              <div className="text-gray-600">${outlier.price.toLocaleString()}</div>
            </div>
          </Popup>
        </Marker>
      ))}

      {/* Individual listing markers (when zoomed in) */}
      {validListings.map((listing) => (
        <Marker
          key={listing.id}
          position={[listing.latitude!, listing.longitude!]}
          icon={createMarkerIcon(listing, listing.id === selectedId)}
          eventHandlers={{
            click: () => onSelect(listing),
          }}
        >
          <Popup>
            <div className="text-sm">
              <div className="font-semibold">{listing.address}</div>
              <div className="text-gray-600">${listing.price.toLocaleString()}</div>
              {listing.amenity_score && (
                <div className="text-xs text-gray-500">
                  Score: {listing.amenity_score.amenity_score}
                </div>
              )}
            </div>
          </Popup>
        </Marker>
      ))}
    </MapContainer>
  );
}
```

**Step 4: Add createOutlierIcon function**

```typescript
function createOutlierIcon(outlier: ClusterOutlier): L.DivIcon {
  const color = getMarkerColor(outlier.amenity_score);
  const size = 12;
  const border = outlier.is_favorite ? '3px solid #FFD700' : '2px solid white';

  return L.divIcon({
    className: 'custom-marker',
    html: `<div style="
      width: ${size}px;
      height: ${size}px;
      background: ${color};
      border-radius: 50%;
      border: ${border};
      box-shadow: 0 2px 4px rgba(0,0,0,0.3);
    "></div>`,
    iconSize: [size, size],
    iconAnchor: [size / 2, size / 2],
  });
}
```

**Step 5: Commit**

```bash
git add frontend/src/components/Map.tsx
git commit -m "feat(map): add cluster and outlier marker rendering"
```

---

## Task 11: Update Dashboard to Use Clusters

**Files:**
- Modify: `frontend/src/pages/Dashboard.tsx`

**Step 1: Rewrite Dashboard**

Replace the contents of `frontend/src/pages/Dashboard.tsx`:

```typescript
import { useState, useCallback } from 'react';
import { useAuth } from '../hooks/useAuth';
import { useListings } from '../hooks/useListings';
import { useClusters } from '../hooks/useClusters';
import { usePersistedFilters } from '../hooks/usePersistedFilters';
import type { Listing, BBox, Cluster, ClusterOutlier } from '../types';
import Map from '../components/Map';
import ListingSidebar from '../components/ListingSidebar';
import ListingDetail from '../components/ListingDetail';
import FilterBar from '../components/FilterBar';

export default function Dashboard() {
  const { user, logout } = useAuth();
  const [filters, setFilters] = usePersistedFilters();
  const [selectedListing, setSelectedListing] = useState<Listing | null>(null);
  const [mapBounds, setMapBounds] = useState<{ bbox: BBox; zoom: number } | null>(null);

  // Fetch clusters based on current viewport
  const {
    data: clusterData,
    isLoading: clustersLoading
  } = useClusters({
    bbox: mapBounds?.bbox ?? null,
    zoom: mapBounds?.zoom ?? 11,
    filters,
    enabled: !!mapBounds,
  });

  // Fetch full listings for expanded cluster view
  const { data: listings = [], isStreaming } = useListings({
    ...filters,
    bbox: mapBounds
      ? `${mapBounds.bbox.minLng},${mapBounds.bbox.minLat},${mapBounds.bbox.maxLng},${mapBounds.bbox.maxLat}`
      : undefined,
  });

  const clusters = clusterData?.clusters ?? [];
  const outliers = clusterData?.outliers ?? [];
  const totalCount = clusters.reduce((sum, c) => sum + c.count, 0) + outliers.length;

  const handleBoundsChange = useCallback((bbox: BBox, zoom: number) => {
    setMapBounds({ bbox, zoom });
  }, []);

  const handleSelect = (item: Listing | ClusterOutlier) => {
    // If it's an outlier, fetch full listing data
    if ('address' in item && !('amenity_score' in item && typeof item.amenity_score === 'object')) {
      const fullListing = listings.find(l => l.id === item.id);
      if (fullListing) {
        setSelectedListing(fullListing);
      }
    } else {
      setSelectedListing(item as Listing);
    }
  };

  const handleClusterClick = (cluster: Cluster) => {
    // Could zoom to cluster bounds here if desired
    console.log('Cluster clicked:', cluster.label);
  };

  return (
    <div className="h-screen flex flex-col">
      {/* Header */}
      <header className="bg-white shadow px-4 py-3 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h1 className="text-xl font-bold">HomeHero</h1>
          {(clustersLoading || isStreaming) && (
            <div className="flex items-center gap-2 text-sm text-blue-600">
              <div className="animate-spin h-4 w-4 border-2 border-blue-600 border-t-transparent rounded-full"></div>
              <span>Loading...</span>
            </div>
          )}
        </div>
        <div className="flex items-center gap-4">
          <FilterBar filters={filters} onChange={setFilters} />
          <span className="text-sm text-gray-600">{user?.name}</span>
          <button
            onClick={logout}
            className="text-sm text-gray-500 hover:text-gray-700"
          >
            Logout
          </button>
        </div>
      </header>

      {/* Main content */}
      <div className="flex-1 flex overflow-hidden">
        {/* Map */}
        <div className="flex-1 relative">
          <Map
            listings={mapBounds && mapBounds.zoom >= 15 ? listings : []}
            clusters={mapBounds && mapBounds.zoom < 15 ? clusters : []}
            outliers={mapBounds && mapBounds.zoom < 15 ? outliers : []}
            selectedId={selectedListing?.id}
            onSelect={handleSelect}
            onBoundsChange={handleBoundsChange}
            onClusterClick={handleClusterClick}
          />
        </div>

        {/* Sidebar */}
        <div className="w-80 bg-white border-l overflow-hidden">
          {selectedListing ? (
            <ListingDetail
              listing={selectedListing}
              onClose={() => setSelectedListing(null)}
            />
          ) : (
            <ListingSidebar
              clusters={clusters}
              outliers={outliers}
              listings={listings}
              isLoading={clustersLoading}
              totalCount={totalCount}
              onSelect={handleSelect}
              onClusterClick={handleClusterClick}
            />
          )}
        </div>
      </div>
    </div>
  );
}
```

**Step 2: Commit**

```bash
git add frontend/src/pages/Dashboard.tsx
git commit -m "feat(dashboard): integrate clusters with viewport-based filtering"
```

---

## Task 12: Test Full Integration

**Step 1: Start backend**

Run: `cd backend && uv run uvicorn app.main:app --reload`

**Step 2: Start frontend**

Run: `cd frontend && npm run dev`

**Step 3: Test viewport filtering**

1. Open http://localhost:5173
2. Login with test credentials
3. Pan/zoom the map
4. Verify sidebar shows cluster cards
5. Click a cluster card to expand
6. Verify virtualized scrolling in expanded view
7. Zoom in to street level (zoom 15+)
8. Verify individual markers appear

**Step 4: Test performance**

1. Zoom out to city-wide view
2. Measure initial load time (should be <1s)
3. Pan around - verify smooth updates
4. Check Network tab - verify small payload sizes

**Step 5: Final commit**

```bash
git add -A
git commit -m "feat: complete viewport-based clustering implementation"
```

---

## Summary

| Task | Component | What It Does |
|------|-----------|--------------|
| 1 | Backend listings | Adds bbox filter to existing endpoints |
| 2 | Backend clusters | New DBSCAN clustering endpoint |
| 3 | Frontend types | TypeScript types for clusters |
| 4 | API client | getClusters method |
| 5 | useClusters hook | React Query hook for clusters |
| 6 | Map bounds | Debounced viewport tracking |
| 7 | Dependencies | react-window, react-leaflet-cluster |
| 8 | ClusterCard | Aggregated area display component |
| 9 | ListingSidebar | Two-mode (aggregated/expanded) sidebar |
| 10 | Map | Cluster markers and outlier markers |
| 11 | Dashboard | Wires everything together |
| 12 | Testing | End-to-end verification |

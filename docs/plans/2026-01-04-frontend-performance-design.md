# Frontend Performance Optimization Design

## Problem

The frontend currently loads and renders all 10k listings upfront, causing slow initial load and sluggish UI. Users don't need all listings at once - they browse by region.

## Solution: Viewport-First Rendering

Only fetch and render listings within the current map viewport. Use clustering for aggregation and density-based grouping in the sidebar.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    Map View                         │
│  ┌─────────────────────────────────────────────┐   │
│  │   [47]  ←── Cluster marker (zoomed out)     │   │
│  │     ↓                                        │   │
│  │   • • •  ←── Individual markers (zoomed in) │   │
│  │                                              │   │
│  │   ▢ ←── User-drawn region (optional)        │   │
│  └─────────────────────────────────────────────┘   │
│                                                     │
│  Sidebar:                                          │
│  ┌──────────────────────┐                          │
│  │ Area 1 (234)         │ ← Aggregated view        │
│  │ $1,650-$2,400 │ 1-3bd│                          │
│  ├──────────────────────┤                          │
│  │ Area 2 (156)         │                          │
│  │ $1,100-$1,600 │ 1-2bd│                          │
│  └──────────────────────┘                          │
│         ↓ click area ↓                             │
│  ┌──────────────────────┐                          │
│  │ ◀ Back │ Area 1 (234)│                          │
│  │ • 123 Main St  $1,750│ ← Virtualized list       │
│  │ • 456 Oak Ave  $1,820│                          │
│  └──────────────────────┘                          │
└─────────────────────────────────────────────────────┘
```

## Map Viewport Filtering

### Behavior

1. User pans/zooms the map
2. After 300ms debounce, extract viewport bounds
3. Send bounds to backend: `GET /api/listings?bbox=minLng,minLat,maxLng,maxLat`
4. Backend returns only listings within bounds (50-500 instead of 10k)

### Draw-to-Filter

- User clicks "Draw Region" button to enter draw mode
- Draws polygon on map using Leaflet.draw
- Polygon coordinates sent as `?polygon=[[lng,lat],[lng,lat],...]`
- Drawn region takes priority over viewport bounds
- Persists until user clears it

### Initial Load

- Map starts at default center/zoom (or last saved position)
- Initial viewport bounds trigger first filtered fetch
- No "loading 10k listings" delay

## Density Clustering

### Algorithm

Server-side DBSCAN-style clustering when returning listings for a viewport.

### API Response Format

```json
{
  "clusters": [
    {
      "id": "cluster_1",
      "center": { "lat": 43.65, "lng": -79.38 },
      "count": 234,
      "bounds": { "north": 43.67, "south": 43.63, "east": -79.35, "west": -79.41 },
      "stats": {
        "price_min": 1650,
        "price_max": 2400,
        "price_avg": 1890,
        "beds_min": 1,
        "beds_max": 3,
        "amenity_avg": 7.2
      },
      "label": "Area 1"
    }
  ],
  "outliers": [
    { "id": 123, "lat": 43.70, "lng": -79.42, "price": 1500, ... }
  ]
}
```

### Clustering by Zoom Level

| Zoom Level | Behavior |
|------------|----------|
| Low (city-wide) | Aggressive clustering, 5-10 large groups |
| Medium | More granular, 10-20 smaller clusters |
| High (neighborhood) | Minimal clustering, mostly individual markers |
| Very high (street) | No clustering, all individual listings |

### Cluster Labels

Simple numeric labels: "Area 1", "Area 2", etc.

## Aggregated Sidebar

### Two Modes

| Mode | When Active | Shows |
|------|-------------|-------|
| Aggregated | Default, or viewport has 50+ listings | Area summaries with stats |
| Expanded | Click on area, or viewport has <50 listings | Virtual-scrolled individual cards |

### Aggregated View

Shows for each cluster:
- Label and count (e.g., "Area 1 (234)")
- Price range
- Bedroom range
- Average amenity score

### Expanded View

- Back button to return to aggregated view
- Virtual-scrolled list of individual listings (only renders visible rows)
- Same ListingCard component as current implementation

## Implementation Changes

### Frontend

| Component | Change |
|-----------|--------|
| `Map.tsx` | Add Leaflet.markercluster, viewport bounds tracking (debounced), Leaflet.draw controls |
| `ListingSidebar.tsx` | Two-mode display (aggregated/expanded), virtual scrolling via react-window |
| `useListings.ts` | Accept bbox/polygon/zoom params, handle cluster response format |
| `Dashboard.tsx` | Coordinate map bounds state with sidebar |
| New: `ClusterCard.tsx` | Component for aggregated area display |

### Backend

| File | Change |
|------|--------|
| `listings.py` | Add `bbox`, `polygon`, `zoom` query params |
| `listings.py` | Spatial filtering with bbox/polygon |
| New: `clustering.py` | DBSCAN clustering with stats aggregation |
| Response format | Return `{ clusters, outliers }` when clustering active |

### New Dependencies

Frontend:
- `react-leaflet-markercluster` - marker clustering on map
- `react-window` - virtual scrolling for expanded list
- `@react-leaflet/core` + `leaflet-draw` - draw polygon controls

Backend:
- `scikit-learn` - DBSCAN algorithm (or simple grid-based clustering)

## Data Flow

```
Map viewport changes (pan/zoom)
    ↓ (300ms debounce)
useListings({ bbox, zoom, polygon?, filters })
    ↓
Backend: spatial filter → cluster if zoom < threshold
    ↓
Response: { clusters, outliers } OR { listings }
    ↓
Map: render cluster markers OR individual markers
Sidebar: render ClusterCards OR virtualized ListingCards
```

## Performance Impact

| Metric | Before | After |
|--------|--------|-------|
| Initial payload | 10k listings | 10-20 clusters |
| DOM nodes (sidebar) | 10k cards | 10-20 cards (aggregated) or ~20 visible (virtualized) |
| Map markers | 10k markers | Clustered markers |
| Filter change | Re-fetch all | Re-fetch viewport only |

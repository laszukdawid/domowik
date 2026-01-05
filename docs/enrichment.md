# Listing Enrichment

Enrichment adds walkability and amenity scores to listings by querying OpenStreetMap data via a local Overpass instance.

## How It Works

```
Listing (lat/lng) → Local Overpass API → Distance calculations → Walkability score
```

1. Extract coordinates from listing's PostGIS location
2. Query local Overpass API for nearby amenities (parallel queries)
3. Calculate distances using Haversine formula
4. Compute walkability score (0-100)
5. Store in `amenity_scores` table

## Setup (One-Time)

Initialize the local Overpass database with BC data (~15 min):

```bash
task overpass:init
```

This downloads the British Columbia OSM extract (~1.1 GB) and indexes it.

## Amenity Types Queried

| Type | Radius | OSM Tags |
|------|--------|----------|
| Parks | 1000m | `leisure=park`, `leisure=garden` |
| Coffee shops | 1000m | `amenity=cafe`, `cuisine=coffee` |
| Dog parks | 2000m | `leisure=dog_park` |

## Scoring Formula

| Component | Points | Criteria |
|-----------|--------|----------|
| **Nearest park** | 0-20 | ≤200m: 20, ≤500m: 15, ≤1000m: 10 |
| Park count bonus | 0-10 | 2 pts per park in 1km (max 10) |
| **Nearest coffee** | 0-15 | ≤150m: 15, ≤300m: 12, ≤500m: 8 |
| Coffee count bonus | 0-10 | 2 pts per cafe in 500m (max 10) |
| **Nearest dog park** | 0-15 | ≤500m: 15, ≤1000m: 10, ≤2000m: 5 |
| **Combined bonus** | 0-20 | All 3 types: 20, Park+Coffee: 10, Either: 5 |
| **Total** | 0-100 | Capped |

## Performance

Using local Overpass with parallel queries:

| Metric | Value |
|--------|-------|
| Per listing | ~100-200ms |
| 100 listings | ~20-30 seconds |
| Storage | ~10 GB (indexed BC data) |

## Running Enrichment

```bash
# Enrich during scraping (automatic)
task scraper:run

# Enrich existing listings without scores
task scraper:enrich

# Check Overpass status
task overpass:status
```

## Code Locations

- `backend/scraper/enricher.py` - Core logic, Overpass queries, score calculation
- `backend/scraper/enrich.py` - Standalone batch runner
- `backend/app/models/amenity.py` - AmenityScore model
- `backend/app/config.py` - OVERPASS_URL setting

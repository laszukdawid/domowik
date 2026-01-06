# Local Overpass API for Fast Enrichment

## Problem

Enrichment is slow due to remote Overpass API rate limiting (1.5s between requests). With 3 queries per listing, each takes ~4.5s minimum. 100 listings = 8-15 minutes.

## Solution

Run local Overpass instance with pre-indexed BC data. No rate limiting, parallel queries, ~100-200ms per listing.

## Architecture

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│    Enricher     │────▶│  Overpass API   │────▶│   OSM Data      │
│  (backend)      │     │  (local:12345)  │     │  (BC extract)   │
└─────────────────┘     └─────────────────┘     └─────────────────┘
                              │
                              ▼
                        Docker volume
                        (overpass-db)
```

- `wiktorn/overpass-api` Docker image
- Pre-baked volume with BC extract (~10 GB indexed)
- Enricher queries `http://overpass:12345/api/interpreter`

## Docker Compose

```yaml
services:
  overpass:
    image: wiktorn/overpass-api
    volumes:
      - overpass-db:/db
    ports:
      - "12345:80"
    environment:
      - OVERPASS_MODE=run
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost/api/status"]
      interval: 30s
      timeout: 10s
      retries: 3

volumes:
  overpass-db:
```

Backend depends on overpass with `condition: service_healthy`.

## Data Initialization

One-time setup (~15 min):

```bash
task overpass:init
```

1. Downloads `british-columbia-latest.osm.pbf` from Geofabrik (~1.1 GB)
2. Runs init container with `OVERPASS_MODE=init` to index
3. Volume persists for subsequent runs

## Enricher Changes

**Config:**
```python
# backend/app/config.py
OVERPASS_URL: str = "http://overpass:12345/api/interpreter"
```

**Remove:**
- `_rate_limit()` method and 1.5s delay
- `_last_request_time` tracking

**Modify:**
- Use `settings.OVERPASS_URL` instead of hardcoded remote
- Reduce timeout from 25s to 5s
- Parallelize all 3 queries with `asyncio.gather()`

```python
async def enrich(self, lat: float, lng: float) -> AmenityData:
    parks, cafes, dog_parks = await asyncio.gather(
        self._query_parks(lat, lng),
        self._query_coffee_shops(lat, lng),
        self._query_dog_parks(lat, lng),
    )
    return self._calculate_scores(parks, cafes, dog_parks)
```

## Taskfile

```yaml
overpass:init:
  desc: Download BC data and initialize Overpass volume (one-time, ~15 min)
  cmds:
    - curl -o /tmp/bc.osm.pbf https://download.geofabrik.de/north-america/canada/british-columbia-latest.osm.pbf
    - docker compose run --rm -e OVERPASS_MODE=init -v /tmp/bc.osm.pbf:/db/region.osm.pbf overpass

overpass:status:
  desc: Check Overpass API status
  cmds:
    - curl http://localhost:12345/api/status
```

## Kubernetes

**k8s/overpass/deployment.yaml:**
- Single replica deployment
- PersistentVolumeClaim (~10 GB)
- Init container downloads and indexes on first deploy
- Service on port 80 (internal only)

No ingress needed - backend accesses via cluster DNS.

## Performance

| Metric | Before | After |
|--------|--------|-------|
| Per-listing | ~4.5s | ~100-200ms |
| 100 listings | 8-15 min | 20-30 sec |
| Rate limiting | Required | None |
| Storage | None | ~10 GB |

## Documentation Updates

- `docs/enrichment.md` - Update with local architecture
- `Claude.md` - Add `task overpass:init` to Quick Commands
- `README.md` - Add overpass init to Quick Start

# Local Overpass Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace remote Overpass API with local instance for 20-50x faster enrichment.

**Architecture:** Add Overpass service to docker-compose with pre-baked BC data volume. Refactor enricher to use configurable URL, remove rate limiting, parallelize queries.

**Tech Stack:** wiktorn/overpass-api Docker image, asyncio.gather for parallel queries

---

### Task 1: Add Overpass Service to Docker Compose

**Files:**
- Modify: `docker-compose.yml`

**Step 1: Add overpass service and volume**

Add to `docker-compose.yml` after the `frontend` service:

```yaml
  overpass:
    image: wiktorn/overpass-api
    volumes:
      - overpass_db:/db
    ports:
      - "12345:80"
    environment:
      - OVERPASS_MODE=run
    healthcheck:
      test: ["CMD", "wget", "-q", "--spider", "http://localhost/api/status"]
      interval: 30s
      timeout: 10s
      retries: 3
```

Add to `volumes:` section:

```yaml
  overpass_db:
```

**Step 2: Update backend depends_on**

Change backend service to:

```yaml
  backend:
    build: ./backend
    ports:
      - "8000:8000"
    environment:
      DATABASE_URL: postgresql+asyncpg://postgres:postgres@db:5432/housesearch
      OVERPASS_URL: http://overpass:80/api/interpreter
    depends_on:
      db:
        condition: service_started
      overpass:
        condition: service_healthy
    volumes:
      - ./backend:/app
    command: uv run uvicorn app.main:app --host 0.0.0.0 --reload
```

**Step 3: Verify syntax**

Run: `docker compose config`
Expected: Valid YAML output, no errors

**Step 4: Commit**

```bash
git add docker-compose.yml
git commit -m "feat: add local overpass service to docker-compose"
```

---

### Task 2: Add Overpass Tasks to Taskfile

**Files:**
- Modify: `Taskfile.yml`

**Step 1: Add overpass tasks section**

Add after the `# Scraper` section:

```yaml
  # =============================================================================
  # Overpass (Local OSM)
  # =============================================================================
  overpass:init:
    desc: "Download BC data and initialize Overpass volume (one-time, ~15 min)"
    cmds:
      - echo "Downloading British Columbia OSM data..."
      - curl -L -o /tmp/bc.osm.pbf https://download.geofabrik.de/north-america/canada/british-columbia-latest.osm.pbf
      - echo "Initializing Overpass database (this takes ~15 minutes)..."
      - "{{.DOCKER_COMPOSE}} run --rm -e OVERPASS_MODE=init -e OVERPASS_PLANET_URL=file:///db/region.osm.pbf -v /tmp/bc.osm.pbf:/db/region.osm.pbf overpass"
      - echo "Done! Overpass is ready to use."

  overpass:status:
    desc: Check Overpass API status
    cmds:
      - curl -s http://localhost:12345/api/status

  overpass:test:
    desc: Test Overpass with a sample query
    cmds:
      - |
        curl -s -X POST http://localhost:12345/api/interpreter \
          -d 'data=[out:json][timeout:5];node["amenity"="cafe"](around:500,49.2827,-123.1207);out;' \
          | head -c 500
```

**Step 2: Verify Taskfile syntax**

Run: `task --list`
Expected: Shows `overpass:init`, `overpass:status`, `overpass:test` in output

**Step 3: Commit**

```bash
git add Taskfile.yml
git commit -m "feat: add overpass init and status tasks"
```

---

### Task 3: Add OVERPASS_URL to Config

**Files:**
- Modify: `backend/app/config.py`

**Step 1: Add overpass_url setting**

```python
from pydantic_settings import BaseSettings


class Settings(BaseSettings):
    database_url: str = "postgresql+asyncpg://postgres:postgres@db:5432/housesearch"
    overpass_url: str = "http://overpass:80/api/interpreter"
    jwt_secret: str = "change-me-in-production"
    jwt_algorithm: str = "HS256"
    jwt_expire_minutes: int = 60 * 24 * 7  # 1 week

    smtp_host: str = "mail.homehero.pro"
    smtp_port: int = 587
    smtp_user: str = "notifications@homehero.pro"
    smtp_pass: str = ""

    class Config:
        env_file = ".env"


settings = Settings()
```

**Step 2: Commit**

```bash
git add backend/app/config.py
git commit -m "feat: add overpass_url config setting"
```

---

### Task 4: Update Enricher Tests for Local Overpass

**Files:**
- Modify: `backend/tests/conftest.py`
- Modify: `backend/tests/scraper/test_enricher.py`

**Step 1: Add local overpass URL fixture to conftest.py**

Add at the end of `backend/tests/conftest.py`:

```python
@pytest.fixture
def local_overpass_url() -> str:
    """Local Overpass API URL for testing."""
    return "http://localhost:12345/api/interpreter"
```

**Step 2: Update test mocks to use dynamic URL**

In `backend/tests/scraper/test_enricher.py`, update the imports at the top:

```python
import pytest
import respx
from httpx import Response

from scraper.enricher import (
    AmenityData,
    AmenityEnricher,
    calculate_walkability_score,
    haversine_distance,
)
from app.config import settings
```

**Step 3: Update TestOverpassParksParsing tests**

Replace the mock URL pattern in each test. Change all occurrences of:

```python
respx.post("https://overpass-api.de/api/interpreter")
```

to:

```python
respx.post(settings.overpass_url)
```

This affects tests in:
- `TestOverpassParksParsing`
- `TestOverpassCoffeeShopsParsing`
- `TestOverpassDogParksParsing`
- `TestOverpassEmptyResponse`
- `TestOverpassErrorHandling`
- `TestEnrichCombined`

**Step 4: Add test for parallel query execution**

Add new test class at the end of the file:

```python
class TestParallelQueries:
    """Tests for parallel query execution."""

    @respx.mock
    async def test_enrich_runs_queries_in_parallel(
        self,
        overpass_parks_response,
        overpass_coffee_shops_response,
        overpass_dog_parks_response,
    ):
        """All three queries should run concurrently."""
        import time

        call_times = []

        def record_call(request):
            call_times.append(time.time())
            # Determine which response to return based on query content
            query = request.content.decode()
            if "leisure" in query and "park" in query and "dog_park" not in query:
                return Response(200, json=overpass_parks_response)
            elif "amenity" in query and "cafe" in query:
                return Response(200, json=overpass_coffee_shops_response)
            else:
                return Response(200, json=overpass_dog_parks_response)

        respx.post(settings.overpass_url).mock(side_effect=record_call)

        enricher = AmenityEnricher()
        try:
            start = time.time()
            await enricher.enrich(49.2827, -123.1207)
            elapsed = time.time() - start

            # All 3 calls should happen nearly simultaneously (within 0.5s of each other)
            assert len(call_times) == 3
            assert max(call_times) - min(call_times) < 0.5
        finally:
            await enricher.close()
```

**Step 5: Run tests to see them fail**

Run: `cd backend && uv run pytest tests/scraper/test_enricher.py -v`
Expected: Tests fail because enricher still uses hardcoded URL and sequential queries

**Step 6: Commit**

```bash
git add backend/tests/conftest.py backend/tests/scraper/test_enricher.py
git commit -m "test: update enricher tests for local overpass and parallel queries"
```

---

### Task 5: Refactor Enricher for Local Overpass

**Files:**
- Modify: `backend/scraper/enricher.py`

**Step 1: Update imports and remove constants**

Replace the top of the file:

```python
"""
Amenity enrichment using OpenStreetMap Overpass API.

Queries nearby parks, coffee shops, and dog parks for each listing.
Uses local Overpass instance for fast queries without rate limiting.
"""

import asyncio
from typing import Any

import httpx
from pydantic import BaseModel

from app.config import settings
```

Remove these lines (no longer needed):
- `OVERPASS_URL = "https://overpass-api.de/api/interpreter"`
- `MAX_RETRIES = 3`
- `RETRY_DELAY_BASE = 5`

**Step 2: Simplify AmenityEnricher.__init__**

Replace:

```python
class AmenityEnricher:
    def __init__(self):
        self.client = httpx.AsyncClient(timeout=10.0)  # Reduced timeout for local
```

Remove:
- `self._last_request_time = 0.0`

**Step 3: Remove _rate_limit method**

Delete the entire `_rate_limit` method (lines 115-122 in original).

**Step 4: Simplify _query_overpass method**

Replace the entire method with:

```python
    async def _query_overpass(self, query: str) -> list[dict]:
        """Execute Overpass query against local instance."""
        try:
            response = await self.client.post(
                settings.overpass_url,
                data={"data": query},
                headers={"Content-Type": "application/x-www-form-urlencoded"},
            )
            response.raise_for_status()
            data = response.json()
            return data.get("elements", [])
        except httpx.HTTPError as e:
            print(f"Overpass query error: {e}")
            return []
```

**Step 5: Update query timeouts**

In `get_nearby_parks`, `get_nearby_coffee_shops`, and `get_nearby_dog_parks`, change:

```python
[out:json][timeout:25];
```

to:

```python
[out:json][timeout:5];
```

**Step 6: Parallelize enrich method**

Replace the `enrich` method:

```python
    async def enrich(self, lat: float, lng: float) -> AmenityData:
        """Get all amenity data for a location."""
        # Run all queries in parallel - no rate limiting with local Overpass
        parks, cafes, dog_parks = await asyncio.gather(
            self.get_nearby_parks(lat, lng),
            self.get_nearby_coffee_shops(lat, lng),
            self.get_nearby_dog_parks(lat, lng),
        )

        data = AmenityData(
            parks=parks[:10],  # Limit to top 10
            coffee_shops=cafes[:10],
            dog_parks=dog_parks[:5],
            nearest_park_m=parks[0]["distance_m"] if parks else None,
            nearest_coffee_m=cafes[0]["distance_m"] if cafes else None,
            nearest_dog_park_m=dog_parks[0]["distance_m"] if dog_parks else None,
        )

        data.walkability_score = calculate_walkability_score(data)
        data.amenity_score = data.walkability_score  # Same for now

        return data
```

**Step 7: Run tests**

Run: `cd backend && uv run pytest tests/scraper/test_enricher.py -v`
Expected: All tests pass

**Step 8: Commit**

```bash
git add backend/scraper/enricher.py
git commit -m "feat: use local overpass with parallel queries"
```

---

### Task 6: Add Kubernetes Overpass Deployment

**Files:**
- Create: `k8s/overpass/pvc.yaml`
- Create: `k8s/overpass/deployment.yaml`
- Create: `k8s/overpass/service.yaml`
- Modify: `k8s/kustomization.yaml`
- Modify: `k8s/backend/deployment.yaml`

**Step 1: Create k8s/overpass directory**

Run: `mkdir -p k8s/overpass`

**Step 2: Create pvc.yaml**

Create `k8s/overpass/pvc.yaml`:

```yaml
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: overpass-data
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 15Gi
```

**Step 3: Create deployment.yaml**

Create `k8s/overpass/deployment.yaml`:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: overpass
spec:
  replicas: 1
  selector:
    matchLabels:
      app: overpass
  template:
    metadata:
      labels:
        app: overpass
    spec:
      initContainers:
        - name: init-data
          image: curlimages/curl:latest
          command:
            - /bin/sh
            - -c
            - |
              if [ ! -f /db/initialized ]; then
                echo "Downloading BC OSM data..."
                curl -L -o /db/region.osm.pbf https://download.geofabrik.de/north-america/canada/british-columbia-latest.osm.pbf
                touch /db/needs_init
              fi
          volumeMounts:
            - name: overpass-data
              mountPath: /db
        - name: init-overpass
          image: wiktorn/overpass-api
          command:
            - /bin/sh
            - -c
            - |
              if [ -f /db/needs_init ]; then
                echo "Initializing Overpass database..."
                /app/bin/init_osm3s.sh /db/region.osm.pbf /db/db /app
                rm /db/needs_init
                touch /db/initialized
              fi
          volumeMounts:
            - name: overpass-data
              mountPath: /db
      containers:
        - name: overpass
          image: wiktorn/overpass-api
          ports:
            - containerPort: 80
          env:
            - name: OVERPASS_MODE
              value: run
          volumeMounts:
            - name: overpass-data
              mountPath: /db
          readinessProbe:
            httpGet:
              path: /api/status
              port: 80
            initialDelaySeconds: 5
            periodSeconds: 10
          resources:
            requests:
              memory: "1Gi"
              cpu: "250m"
            limits:
              memory: "2Gi"
              cpu: "1000m"
      volumes:
        - name: overpass-data
          persistentVolumeClaim:
            claimName: overpass-data
```

**Step 4: Create service.yaml**

Create `k8s/overpass/service.yaml`:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: overpass
spec:
  selector:
    app: overpass
  ports:
    - port: 80
      targetPort: 80
```

**Step 5: Update kustomization.yaml**

Add overpass resources to `k8s/kustomization.yaml`:

```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

namespace: domowik

resources:
  - namespace.yaml
  - secrets.yaml
  - postgres/pvc.yaml
  - postgres/deployment.yaml
  - postgres/service.yaml
  - overpass/pvc.yaml
  - overpass/deployment.yaml
  - overpass/service.yaml
  - backend/deployment.yaml
  - backend/service.yaml
  - frontend/deployment.yaml
  - frontend/service.yaml
  - scraper/cronjob.yaml
  - ingress.yaml
```

**Step 6: Update backend deployment with OVERPASS_URL**

Read current `k8s/backend/deployment.yaml` and add OVERPASS_URL env var to the container:

```yaml
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: domowik-secrets
                  key: database-url
            - name: OVERPASS_URL
              value: "http://overpass:80/api/interpreter"
```

**Step 7: Verify kustomize**

Run: `kubectl kustomize k8s/`
Expected: Valid YAML output with overpass resources

**Step 8: Commit**

```bash
git add k8s/
git commit -m "feat: add overpass kubernetes deployment"
```

---

### Task 7: Update Documentation

**Files:**
- Modify: `docs/enrichment.md`
- Modify: `Claude.md`
- Modify: `README.md`

**Step 1: Update docs/enrichment.md**

Replace the entire file with:

```markdown
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
```

**Step 2: Update Claude.md Quick Commands**

Add to the Quick Commands section:

```markdown
- `task overpass:init` - Initialize local Overpass (one-time, ~15 min)
- `task overpass:status` - Check Overpass API status
```

**Step 3: Update README.md Quick Start**

Update the Quick Start section:

```markdown
## Quick Start

```bash
# 1. Start all services
task up

# 2. Initialize Overpass (first time only, ~15 min)
task overpass:init

# 3. Run database migrations (required on first run!)
task db:migrate

# 4. Access the app
# Frontend: http://localhost:3000
# Backend API: http://localhost:8000
# API docs: http://localhost:8000/docs
```
```

**Step 4: Commit**

```bash
git add docs/enrichment.md Claude.md README.md
git commit -m "docs: update enrichment docs for local overpass"
```

---

### Task 8: Integration Test

**Step 1: Start services**

Run: `task up`
Expected: All services start, overpass shows as healthy

**Step 2: Initialize Overpass (if not done)**

Run: `task overpass:init`
Expected: Downloads BC data and initializes (takes ~15 min first time)

**Step 3: Test Overpass status**

Run: `task overpass:status`
Expected: Returns status JSON

**Step 4: Test sample query**

Run: `task overpass:test`
Expected: Returns JSON with cafe nodes near Vancouver downtown

**Step 5: Run enricher tests**

Run: `cd backend && uv run pytest tests/scraper/test_enricher.py -v`
Expected: All tests pass

**Step 6: Test enrichment on real listing**

Run: `task scraper:enrich`
Expected: Enriches listings much faster than before (~100-200ms per listing)

---

## Summary

After completing all tasks:
- Local Overpass runs in Docker with pre-indexed BC data
- Enricher uses configurable URL from settings
- All 3 amenity queries run in parallel
- Performance: ~4.5s → ~100-200ms per listing (20-50x faster)
- K8s deployment ready with init containers for data setup

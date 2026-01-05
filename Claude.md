# Domowik - Ideal House Finder for GVA

Find the perfect home by scoring listings on walkability, amenities, and livability - not just filtering by price and bedrooms.

## Quick Commands
- `task up` / `task down` - Start/stop all services
- `task dev` - Dev mode with logs attached
- `task db:shell` - PostgreSQL shell
- `task scraper:run` - Run MLS scraper
- `task backend:test` - Run pytest
- `task overpass:init` - Initialize local Overpass (one-time, ~15 min)
- `task overpass:status` - Check Overpass API status

## Python
Use `uv run` instead of global python:
- `uv run python` not `python3`
- `uv run pytest` not `python -m pytest`
- Backend dir: `cd backend && uv sync` for deps

## Stack
- **Backend**: FastAPI + SQLAlchemy (async) + PostgreSQL/PostGIS
- **Frontend**: React + TypeScript + Vite + Leaflet + TailwindCSS
- **Infra**: Docker Compose (dev), Kubernetes (prod)
- **Tools**: uv (Python), npm (Node), Task (commands)

## Key Patterns
- PostGIS for spatial queries (bounding box, distance calculations)
- Viewport-based clustering - fetch clusters when zoomed out, full listings when zoomed in
- AmenityScore model holds walkability metrics (park distance, coffee shops, dog parks)
- Scores are 0-100, color-coded on map (green=high, red=low)

## Domain
- Scrapes realtor.ca via pyRealtor for Greater Vancouver Area
- Enriches listings with OpenStreetMap amenity data - see [docs/enrichment.md](docs/enrichment.md)
- MLS ID is the unique identifier for listings (`mls_id` field)
- Listings have `first_seen`/`last_seen` timestamps (UTC) for tracking freshness

## Code Conventions
- Backend: async everywhere, Pydantic schemas in `schemas/`, SQLAlchemy models in `models/`
- Frontend: custom hooks in `hooks/`, API calls in `api/client.ts`
- Migrations: `task db:migrate:create -- 'description'` then `task db:migrate`
- Tests: `backend/tests/`, run with `uv run pytest` from backend dir

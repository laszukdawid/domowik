# HomeHero: Greater Vancouver House Search

## Overview

A web application to help find the perfect house in the Greater Vancouver Area. The system scrapes MLS listings, enriches them with amenity data, and presents matches on an interactive map with collaborative features.

**Key features:**
- Automated daily scraping of realtor.ca listings
- Amenity scoring (parks, coffee shops, dog parks, walkability)
- Interactive map with filtering
- Multi-user support with notes and favorites
- Email notifications for new matching listings

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        house-search                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐     ┌──────────────┐     ┌──────────────┐     │
│  │   Scraper    │────▶│  PostgreSQL  │◀────│   FastAPI    │     │
│  │  (CronJob)   │     │  + PostGIS   │     │   Backend    │     │
│  └──────────────┘     └──────────────┘     └──────────────┘     │
│         │                    │                    ▲              │
│         ▼                    │                    │              │
│  ┌──────────────┐            │             ┌──────────────┐     │
│  │  Amenities   │────────────┘             │    React     │     │
│  │   Enricher   │                          │  + Leaflet   │     │
│  └──────────────┘                          └──────────────┘     │
│         │                                                        │
│         ▼                                                        │
│  ┌──────────────┐                                               │
│  │    Email     │                                               │
│  │   Notifier   │                                               │
│  └──────────────┘                                               │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

**Components:**
- **Scraper** - Kubernetes CronJob, pulls listings from realtor.ca, detects new/changed/removed listings
- **Amenities Enricher** - Queries OpenStreetMap for nearby parks, coffee shops, dog parks; scores each property
- **PostgreSQL + PostGIS** - Stores listings, amenity scores, user notes, spatial queries
- **FastAPI Backend** - REST API for the frontend, handles filtering, notes, favorites, auth
- **React + Leaflet Frontend** - Interactive map, property cards, filtering controls, note-taking
- **Email Notifier** - Sends digest emails via SMTP (homehero.pro) when new matching listings appear

## Data Model

### users
| Column | Type | Description |
|--------|------|-------------|
| id | PK | Internal ID |
| email | string | Login email (unique) |
| name | string | Display name |
| password_hash | string | Hashed password |
| created_at | timestamp | Account creation time |

### user_preferences
| Column | Type | Description |
|--------|------|-------------|
| user_id | FK | Links to user |
| min_price | integer | Price range filter |
| max_price | integer | Price range filter |
| min_bedrooms | integer | Minimum beds |
| min_sqft | integer | Minimum square footage |
| cities | array | Cities to include |
| property_types | array | Types (house, townhouse, etc.) |
| max_park_distance | integer | Must have park within X meters |
| notify_email | boolean | Send email alerts |

### listings
| Column | Type | Description |
|--------|------|-------------|
| id | PK | Internal ID |
| mls_id | string | MLS listing number (unique) |
| url | string | Link to realtor.ca listing |
| address | string | Full address string |
| city | string | City/municipality |
| location | PostGIS POINT | Lat/lng for spatial queries |
| price | integer | Listing price in CAD |
| bedrooms | integer | Number of bedrooms |
| bathrooms | integer | Number of bathrooms |
| sqft | integer | Square footage |
| property_type | string | House, townhouse, condo, etc. |
| listing_date | date | When it was listed |
| first_seen | timestamp | When we first scraped it |
| last_seen | timestamp | Last time we saw it in scrape |
| status | string | active / sold / delisted |
| raw_data | JSONB | Full scraped data |

### amenity_scores
| Column | Type | Description |
|--------|------|-------------|
| listing_id | FK | Links to listing |
| nearest_park_m | integer | Distance to nearest park in meters |
| nearest_coffee_m | integer | Distance to nearest coffee shop |
| nearest_dog_park_m | integer | Distance to nearest dog park |
| parks | JSONB | Array of nearby parks with distances |
| coffee_shops | JSONB | Array of nearby coffee shops |
| walkability_score | integer | 0-100 based on amenity density |
| amenity_score | integer | Computed overall score (0-100) |

### user_notes
| Column | Type | Description |
|--------|------|-------------|
| id | PK | Note ID |
| listing_id | FK | Links to listing |
| user_id | FK | Who wrote it |
| note | text | Free text note |
| created_at | timestamp | Timestamp |

### user_listing_status
| Column | Type | Description |
|--------|------|-------------|
| user_id | FK | Links to user |
| listing_id | FK | Links to listing |
| is_favorite | boolean | Starred/saved |
| is_hidden | boolean | Dismissed from view |
| viewed_at | timestamp | When first viewed in UI |

## Scraper Design

**Approach:**
- Query realtor.ca with Greater Vancouver Area bounds
- Use their search API (JSON responses) rather than HTML scraping
- Handle pagination (typically 20-50 results per page)
- Run daily via Kubernetes CronJob

**Change detection:**
- Compare `mls_id` against existing listings
- New listing: insert, mark for amenity enrichment, flag for notification
- Existing listing: update price/status if changed
- Missing from scrape: mark as delisted after 2 consecutive misses

**Rate limiting:**
- 2-3 second delay between requests
- Rotate user agents
- Respect robots.txt

**Tech:**
- Python with `httpx` for async requests
- `pydantic` for parsing/validation

## Amenity Enrichment

**Data sources:**
- Primary: OpenStreetMap via Overpass API (free)
- Extensible: Google Places API can be added later

**What we query:**
| Amenity Type | OSM Tags | Radius |
|--------------|----------|--------|
| Parks | `leisure=park`, `leisure=garden` | 1km |
| Dog Parks | `leisure=dog_park` | 2km |
| Coffee Shops | `amenity=cafe`, `cuisine=coffee` | 1km |

**Stored data per listing:**
```json
{
  "parks": [
    {"name": "Queen Elizabeth Park", "distance_m": 340, "area_sqm": 52000},
    {"name": "Riley Park", "distance_m": 780, "area_sqm": 12000}
  ],
  "coffee_shops": [
    {"name": "49th Parallel", "distance_m": 210},
    {"name": "Starbucks", "distance_m": 450}
  ]
}
```

**Walkability scoring (0-100):**
- Parks within 500m: +20 points (max)
- Coffee shop within 300m: +15 points
- Dog park within 1km: +15 points
- Multiple amenities of each type: bonus points up to cap

**Extensibility:**
- Amenity provider abstracted behind interface
- Can add Google Places provider for restaurants, grocery stores, transit

## Email Notifications

**Trigger:**
- Runs after scraper + enrichment complete
- Only processes listings marked as new since last notification

**Matching:**
- For each user with `notify_email = true`
- Filter new listings against their `user_preferences`
- Price range, bedrooms, sqft, cities, property types
- Amenity requirements (e.g., park within 500m)

**Email content:**
- Subject: "3 new listings match your search"
- HTML email with listing cards (photo, address, price, key stats, amenity highlights)
- Direct link to listing in the app

**Configuration:**
- SMTP via homehero.pro mail server
- Sender: notifications@homehero.pro
- Daily digest (not per-listing)

**In-app fallback:**
- Dashboard shows "New since your last visit" section
- Same matching logic as email
- Listings sorted by amenity score
- Badge/count indicator for unread new listings

## Frontend Design

```
┌─────────────────────────────────────────────────────────────────┐
│  Header: HomeHero.pro                    [Filters] [User Menu]  │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────────────────────────┐  ┌──────────────────────┐  │
│  │                                 │  │ New Since Last Visit │  │
│  │                                 │  │ ┌──────────────────┐ │  │
│  │                                 │  │ │ 123 Main St      │ │  │
│  │         Leaflet Map             │  │ │ $1.2M - 3bd 2ba  │ │  │
│  │                                 │  │ │ * 85 amenity     │ │  │
│  │    - = listing (color=score)    │  │ └──────────────────┘ │  │
│  │    * = favorite                 │  │ ┌──────────────────┐ │  │
│  │                                 │  │ │ 456 Oak Ave      │ │  │
│  │                                 │  │ │ $980K - 4bd 3ba  │ │  │
│  └─────────────────────────────────┘  │ │ * 72 amenity     │ │  │
│                                       │ └──────────────────┘ │  │
│  ┌─────────────────────────────────┐  │                      │  │
│  │ Selected: 123 Main St           │  │ All Listings         │  │
│  │ $1,200,000 - 3 bed - 2 bath     │  │ [sorted by score]    │  │
│  │ 1,850 sqft - House              │  │                      │  │
│  │                                 │  │                      │  │
│  │ Park: 120m (Queen E. Park)      │  │                      │  │
│  │ Coffee: 85m (49th Parallel)     │  │                      │  │
│  │ Dog Park: 450m                  │  │                      │  │
│  │ Walkability: 88/100             │  │                      │  │
│  │                                 │  └──────────────────────┘  │
│  │ [Favorite] [Hide] [realtor.ca]  │                            │
│  │                                 │                            │
│  │ Notes:                          │                            │
│  │ ┌─────────────────────────────┐ │                            │
│  │ │ Dawid: Nice backyard!       │ │                            │
│  │ │ Partner: Too close to busy  │ │                            │
│  │ │          street             │ │                            │
│  │ └─────────────────────────────┘ │                            │
│  │ [Add note...]                   │                            │
│  └─────────────────────────────────┘                            │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

**Key views:**
- Map with markers colored by amenity score (green=high, red=low)
- Listing sidebar with new listings section + full list
- Detail panel with amenity breakdown, notes, actions

**Filter bar:**
- Price range slider
- Bedrooms/bathrooms
- Property type checkboxes
- City multi-select
- Amenity requirements

**Tech:**
- React 18 with TypeScript
- Leaflet + react-leaflet for maps
- Tailwind CSS for styling
- React Query for API data fetching

## Project Structure

```
house-search/
├── backend/
│   ├── app/
│   │   ├── api/              # FastAPI routes
│   │   │   ├── auth.py       # Login, register, JWT tokens
│   │   │   ├── listings.py   # CRUD, filtering, search
│   │   │   ├── notes.py      # User notes
│   │   │   └── preferences.py# User preferences
│   │   ├── models/           # SQLAlchemy models
│   │   ├── services/         # Business logic
│   │   │   ├── amenity_provider.py  # Base interface
│   │   │   ├── osm_provider.py      # OpenStreetMap impl
│   │   │   └── scoring.py           # Walkability calc
│   │   ├── config.py         # Settings, env vars
│   │   └── main.py           # FastAPI app
│   ├── scraper/
│   │   ├── realtor_ca.py     # Scraper implementation
│   │   ├── enricher.py       # Amenity enrichment
│   │   └── notifier.py       # Email notifications
│   ├── alembic/              # Database migrations
│   ├── requirements.txt
│   └── Dockerfile
├── frontend/
│   ├── src/
│   │   ├── components/       # React components
│   │   ├── pages/            # Login, Dashboard
│   │   ├── hooks/            # React Query hooks
│   │   ├── api/              # API client
│   │   └── types/            # TypeScript types
│   ├── package.json
│   └── Dockerfile
├── k8s/                      # Kubernetes manifests
│   ├── backend/
│   ├── frontend/
│   ├── scraper/              # CronJob
│   └── ingress.yaml
├── docker-compose.yml        # Local development
├── .env.example
└── README.md
```

## Tech Stack

| Layer | Technology |
|-------|------------|
| Database | PostgreSQL 16 + PostGIS |
| Backend | Python 3.12, FastAPI, SQLAlchemy, Pydantic |
| Auth | JWT tokens |
| Scraper | httpx, runs as K8s CronJob |
| Frontend | React 18, TypeScript, Leaflet, Tailwind |
| Email | SMTP via homehero.pro |
| Dev Environment | Docker Compose |
| Production | Kubernetes |

## Deployment

### Local Development
- `docker-compose up` starts PostgreSQL, backend, frontend
- Hot reload for both backend and frontend
- PostgreSQL data persisted in Docker volume

### Production (Kubernetes)
- Helm chart or Kustomize manifests
- Separate deployments: backend, frontend
- Scraper as CronJob (daily)
- PostgreSQL via managed service or StatefulSet
- Ingress with cert-manager for SSL

### Container Images (optimized for size)

**Backend (~100-150MB):**
```dockerfile
FROM python:3.12-slim AS builder
# install deps, compile

FROM python:3.12-slim
# copy only runtime artifacts
```

**Frontend (~25MB):**
```dockerfile
FROM node:20-alpine AS builder
# npm ci, npm run build

FROM nginx:alpine
# copy dist/ only
```

### Environment Variables
```
DATABASE_URL=postgresql://user:pass@db:5432/housesearch
JWT_SECRET=<random-secret>
SMTP_HOST=mail.homehero.pro
SMTP_USER=notifications@homehero.pro
SMTP_PASS=<password>
```

### Backup
- Daily pg_dump to cloud storage
- Automated via CronJob

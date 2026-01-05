# Domowik - House Search for Greater Vancouver Area

A house search application that scrapes MLS listings from realtor.ca, enriches them with amenity data from OpenStreetMap, and displays results on an interactive map.

## Features

- Daily scraping of realtor.ca listings
- Amenity enrichment (parks, coffee shops, dog parks) via OpenStreetMap - see [docs/enrichment.md](docs/enrichment.md)
- Walkability score calculation (0-100)
- Interactive Leaflet map with filtering
- Multi-user support with notes, favorites, and hide functionality
- Email notifications for new matching listings

## Tech Stack

- **Backend**: FastAPI, SQLAlchemy (async), PostgreSQL + PostGIS
- **Frontend**: React 18, TypeScript, Vite, Leaflet, Tailwind CSS
- **Scraper**: Python with httpx, runs as Kubernetes CronJob in production

## Prerequisites

- Docker and Docker Compose
- [Task](https://taskfile.dev/) (task runner) - `brew install go-task` or `go install github.com/go-task/task/v3/cmd/task@latest`

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

## Environment Variables

Create a `.env` file in the project root (optional for local dev, defaults are provided):

```env
# Database (defaults work with docker-compose)
DATABASE_URL=postgresql+asyncpg://postgres:postgres@db:5432/housesearch

# JWT Authentication
JWT_SECRET=your-secret-key-change-in-production
JWT_ALGORITHM=HS256
JWT_EXPIRE_MINUTES=10080

# Email notifications (optional)
SMTP_HOST=smtp.example.com
SMTP_PORT=587
SMTP_USER=
SMTP_PASSWORD=
SMTP_FROM=noreply@homehero.pro
```

## Available Tasks

Run `task --list` to see all available commands:

### Full Stack
| Command | Description |
|---------|-------------|
| `task up` | Start all services (db, backend, frontend) |
| `task down` | Stop all services |
| `task logs` | Tail logs for all services |
| `task restart` | Restart all services |
| `task build` | Build all Docker images |
| `task clean` | Stop services and remove volumes |

### Database
| Command | Description |
|---------|-------------|
| `task db:up` | Start only the database |
| `task db:logs` | Tail database logs |
| `task db:shell` | Open psql shell |
| `task db:migrate` | Run database migrations |
| `task db:migrate:create -- 'name'` | Create a new migration |

### Backend
| Command | Description |
|---------|-------------|
| `task backend:up` | Start backend and database |
| `task backend:logs` | Tail backend logs |
| `task backend:shell` | Open shell in backend container |
| `task backend:build` | Build backend Docker image |
| `task backend:restart` | Restart backend service |

### Frontend
| Command | Description |
|---------|-------------|
| `task frontend:up` | Start frontend only |
| `task frontend:logs` | Tail frontend logs |
| `task frontend:build` | Build frontend Docker image |
| `task frontend:restart` | Restart frontend service |
| `task frontend:dev` | Run frontend locally with npm |

### Scraper
| Command | Description |
|---------|-------------|
| `task scraper:run` | Run the scraper manually (in Docker) |
| `task scraper:run:local` | Run scraper locally (requires venv) |

### Development
| Command | Description |
|---------|-------------|
| `task dev` | Start all services with logs attached |
| `task dev:backend` | Start db + backend with logs |
| `task dev:frontend` | Start frontend with logs |

## Troubleshooting

### Backend crashes on startup
Check logs with `task backend:logs`. Common issues:

1. **Database not ready**: Wait a few seconds and restart backend
   ```bash
   task backend:restart
   ```

2. **Missing migrations**: Run migrations
   ```bash
   task db:migrate
   ```

### Frontend not accessible
- Ensure you're accessing `http://localhost:3000` (not 3001 or other ports)
- Check if container is running: `docker compose ps`

### Database connection errors
- Ensure database is running: `task db:up`
- Check database logs: `task db:logs`

## Production Deployment (Kubernetes)

```bash
# Build production images
task docker:build

# Push to registry
task docker:push

# Apply Kubernetes manifests
task k8s:apply

# Check status
task k8s:status
```

Kubernetes resources are in the `k8s/` directory and use the `domowik` namespace.

## Project Structure

```
.
├── backend/
│   ├── app/
│   │   ├── api/          # FastAPI routers
│   │   ├── models/       # SQLAlchemy models
│   │   ├── services/     # Business logic
│   │   └── config.py     # Settings
│   ├── scraper/          # Realtor.ca scraper
│   ├── alembic/          # Database migrations
│   └── Dockerfile
├── frontend/
│   ├── src/
│   │   ├── components/   # React components
│   │   ├── hooks/        # Custom hooks
│   │   ├── services/     # API client
│   │   └── types/        # TypeScript types
│   └── Dockerfile
├── k8s/                  # Kubernetes manifests
├── docker-compose.yml
├── Taskfile.yml
└── README.md
```

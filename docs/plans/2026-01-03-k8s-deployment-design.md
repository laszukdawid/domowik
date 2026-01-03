# Kubernetes Deployment Design

**Date:** 2026-01-03
**Domain:** domowik.lasz.uk
**Cluster:** DigitalOcean Kubernetes (DOKS)

## Architecture

```
                    ┌─────────────────┐
                    │   FastMail DNS  │
                    │ domowik.lasz.uk │
                    │ → 157.230.70.84 │
                    └────────┬────────┘
                             │
                    ┌────────▼────────┐
                    │  DOKS Ingress   │
                    │  (nginx + TLS)  │
                    └────────┬────────┘
                             │
              ┌──────────────┴──────────────┐
              │                             │
       /api/* │                             │ /*
              │                             │
     ┌────────▼────────┐          ┌────────▼────────┐
     │     Backend     │          │    Frontend     │
     │   (2 replicas)  │          │   (2 replicas)  │
     │    FastAPI      │          │   Nginx + React │
     └────────┬────────┘          └─────────────────┘
              │
     ┌────────▼────────┐
     │   PostgreSQL    │
     │   + PostGIS     │
     │  (PVC storage)  │
     └─────────────────┘

     ┌─────────────────┐
     │  Scraper Cron   │
     │  (6 AM UTC)     │
     └─────────────────┘
```

## Components

| Component | Image | Replicas | Storage |
|-----------|-------|----------|---------|
| Backend | ghcr.io/laszukdawid/domowik-backend | 2 | - |
| Frontend | ghcr.io/laszukdawid/domowik-frontend | 2 | - |
| PostgreSQL | postgis/postgis:16-3.4-alpine | 1 | 5Gi PVC |
| Scraper | ghcr.io/laszukdawid/domowik-backend | CronJob | - |

## CI/CD

- **Registry:** GitHub Container Registry (ghcr.io)
- **Trigger:** Push to `main` branch
- **Deployment:** Manual `kubectl apply -k k8s/`

## Deployment Steps

1. **DNS Setup (FastMail)**
   - Add A record: `domowik` → `157.230.70.84`

2. **First Deployment**
   ```bash
   # Push to main to trigger image build
   git push origin main

   # Wait for GitHub Actions to complete, then:
   kubectl apply -k k8s/

   # Run migrations
   kubectl exec -n domowik deploy/backend -- alembic upgrade head
   ```

3. **Verify**
   ```bash
   kubectl get pods -n domowik
   kubectl get ingress -n domowik
   ```

## Future Improvements

- Migrate to DigitalOcean Managed Database when scale requires
- Add GitHub Actions auto-deploy with KUBE_CONFIG secret
- Add monitoring (Prometheus/Grafana)

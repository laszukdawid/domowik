from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from app.api import (
    auth_router,
    listings_router,
    notes_router,
    status_router,
    preferences_router,
    clusters_router,
)

app = FastAPI(title="HomeHero API", version="0.1.0")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:3000", "http://localhost:3001"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


app.include_router(auth_router, prefix="/api")
app.include_router(listings_router, prefix="/api")
app.include_router(notes_router, prefix="/api")
app.include_router(status_router, prefix="/api")
app.include_router(preferences_router, prefix="/api")
app.include_router(clusters_router, prefix="/api")


@app.get("/health")
async def health():
    return {"status": "ok"}

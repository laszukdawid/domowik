from app.api.auth import router as auth_router
from app.api.listings import router as listings_router
from app.api.notes import router as notes_router
from app.api.status import router as status_router
from app.api.preferences import router as preferences_router
from app.api.clusters import router as clusters_router
from app.api.pois import router as pois_router
from app.api.custom_lists import router as custom_lists_router

__all__ = [
    "auth_router",
    "listings_router",
    "notes_router",
    "status_router",
    "preferences_router",
    "clusters_router",
    "pois_router",
    "custom_lists_router",
]

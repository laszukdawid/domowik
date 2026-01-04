"""
Amenity enrichment using OpenStreetMap Overpass API.

Queries nearby parks, coffee shops, and dog parks for each listing.
"""

import asyncio
import random
from typing import Any

import httpx
from pydantic import BaseModel


OVERPASS_URL = "https://overpass-api.de/api/interpreter"

# Retry settings for Overpass API
MAX_RETRIES = 3
RETRY_DELAY_BASE = 5  # Base delay in seconds, increases exponentially


class AmenityData(BaseModel):
    nearest_park_m: int | None = None
    nearest_coffee_m: int | None = None
    nearest_dog_park_m: int | None = None
    parks: list[dict[str, Any]] = []
    coffee_shops: list[dict[str, Any]] = []
    dog_parks: list[dict[str, Any]] = []
    walkability_score: int = 0
    amenity_score: int = 0


def haversine_distance(lat1: float, lon1: float, lat2: float, lon2: float) -> float:
    """Calculate distance between two points in meters."""
    from math import radians, sin, cos, sqrt, atan2

    R = 6371000  # Earth's radius in meters

    lat1, lon1, lat2, lon2 = map(radians, [lat1, lon1, lat2, lon2])
    dlat = lat2 - lat1
    dlon = lon2 - lon1

    a = sin(dlat / 2) ** 2 + cos(lat1) * cos(lat2) * sin(dlon / 2) ** 2
    c = 2 * atan2(sqrt(a), sqrt(1 - a))

    return R * c


def calculate_walkability_score(amenity_data: AmenityData) -> int:
    """Calculate walkability score (0-100) based on amenity proximity."""
    score = 0

    # Parks within 500m: up to 20 points
    if amenity_data.nearest_park_m is not None:
        if amenity_data.nearest_park_m <= 200:
            score += 20
        elif amenity_data.nearest_park_m <= 500:
            score += 15
        elif amenity_data.nearest_park_m <= 1000:
            score += 10

    # Multiple parks bonus: up to 10 points
    parks_within_1km = len(
        [p for p in amenity_data.parks if p.get("distance_m", 9999) <= 1000]
    )
    score += min(parks_within_1km * 2, 10)

    # Coffee shops within 300m: up to 15 points
    if amenity_data.nearest_coffee_m is not None:
        if amenity_data.nearest_coffee_m <= 150:
            score += 15
        elif amenity_data.nearest_coffee_m <= 300:
            score += 12
        elif amenity_data.nearest_coffee_m <= 500:
            score += 8

    # Multiple coffee shops bonus: up to 10 points
    cafes_within_500m = len(
        [c for c in amenity_data.coffee_shops if c.get("distance_m", 9999) <= 500]
    )
    score += min(cafes_within_500m * 2, 10)

    # Dog park within 1km: up to 15 points
    if amenity_data.nearest_dog_park_m is not None:
        if amenity_data.nearest_dog_park_m <= 500:
            score += 15
        elif amenity_data.nearest_dog_park_m <= 1000:
            score += 10
        elif amenity_data.nearest_dog_park_m <= 2000:
            score += 5

    # Bonus for having all amenity types nearby: up to 20 points
    has_park = amenity_data.nearest_park_m is not None and amenity_data.nearest_park_m <= 1000
    has_coffee = amenity_data.nearest_coffee_m is not None and amenity_data.nearest_coffee_m <= 500
    has_dog_park = amenity_data.nearest_dog_park_m is not None and amenity_data.nearest_dog_park_m <= 2000

    if has_park and has_coffee and has_dog_park:
        score += 20
    elif has_park and has_coffee:
        score += 10
    elif has_park or has_coffee:
        score += 5

    return min(score, 100)


class AmenityEnricher:
    def __init__(self):
        self.client = httpx.AsyncClient(timeout=60.0)
        self._last_request_time = 0.0

    async def close(self):
        await self.client.aclose()

    async def _rate_limit(self):
        """Ensure minimum delay between requests to Overpass API."""
        import time
        min_delay = 1.5  # Minimum 1.5 seconds between requests
        elapsed = time.time() - self._last_request_time
        if elapsed < min_delay:
            await asyncio.sleep(min_delay - elapsed)
        self._last_request_time = time.time()

    async def _query_overpass(self, query: str) -> list[dict]:
        """Execute Overpass query with retry logic for rate limits and timeouts."""
        for attempt in range(MAX_RETRIES):
            await self._rate_limit()
            try:
                response = await self.client.post(
                    OVERPASS_URL,
                    data={"data": query},
                    headers={"Content-Type": "application/x-www-form-urlencoded"},
                )
                response.raise_for_status()
                data = response.json()
                return data.get("elements", [])
            except httpx.HTTPStatusError as e:
                status = e.response.status_code
                if status in (429, 504, 503, 502) and attempt < MAX_RETRIES - 1:
                    # Rate limited or server overloaded - back off exponentially
                    delay = RETRY_DELAY_BASE * (2 ** attempt) + random.uniform(0, 2)
                    print(f"Overpass {status} error, retrying in {delay:.1f}s (attempt {attempt + 1}/{MAX_RETRIES})")
                    await asyncio.sleep(delay)
                    continue
                print(f"Overpass query error: {e}")
                return []
            except httpx.TimeoutException:
                if attempt < MAX_RETRIES - 1:
                    delay = RETRY_DELAY_BASE * (2 ** attempt) + random.uniform(0, 2)
                    print(f"Overpass timeout, retrying in {delay:.1f}s (attempt {attempt + 1}/{MAX_RETRIES})")
                    await asyncio.sleep(delay)
                    continue
                print("Overpass query timed out after all retries")
                return []
            except httpx.HTTPError as e:
                print(f"Overpass query error: {e}")
                return []
        return []

    async def get_nearby_parks(
        self, lat: float, lng: float, radius_m: int = 1000
    ) -> list[dict]:
        """Get parks within radius."""
        query = f"""
        [out:json][timeout:25];
        (
          way["leisure"="park"](around:{radius_m},{lat},{lng});
          relation["leisure"="park"](around:{radius_m},{lat},{lng});
          way["leisure"="garden"](around:{radius_m},{lat},{lng});
        );
        out center;
        """

        elements = await self._query_overpass(query)
        parks = []

        for el in elements:
            center = el.get("center", {})
            el_lat = center.get("lat") or el.get("lat")
            el_lng = center.get("lon") or el.get("lon")

            if el_lat and el_lng:
                distance = haversine_distance(lat, lng, el_lat, el_lng)
                parks.append(
                    {
                        "name": el.get("tags", {}).get("name", "Unnamed Park"),
                        "distance_m": int(distance),
                        "lat": el_lat,
                        "lng": el_lng,
                    }
                )

        return sorted(parks, key=lambda x: x["distance_m"])

    async def get_nearby_coffee_shops(
        self, lat: float, lng: float, radius_m: int = 1000
    ) -> list[dict]:
        """Get coffee shops within radius."""
        query = f"""
        [out:json][timeout:25];
        (
          node["amenity"="cafe"](around:{radius_m},{lat},{lng});
          way["amenity"="cafe"](around:{radius_m},{lat},{lng});
          node["cuisine"="coffee"](around:{radius_m},{lat},{lng});
        );
        out center;
        """

        elements = await self._query_overpass(query)
        cafes = []

        for el in elements:
            center = el.get("center", {})
            el_lat = center.get("lat") or el.get("lat")
            el_lng = center.get("lon") or el.get("lon")

            if el_lat and el_lng:
                distance = haversine_distance(lat, lng, el_lat, el_lng)
                cafes.append(
                    {
                        "name": el.get("tags", {}).get("name", "Unnamed Cafe"),
                        "distance_m": int(distance),
                        "lat": el_lat,
                        "lng": el_lng,
                    }
                )

        return sorted(cafes, key=lambda x: x["distance_m"])

    async def get_nearby_dog_parks(
        self, lat: float, lng: float, radius_m: int = 2000
    ) -> list[dict]:
        """Get dog parks within radius."""
        query = f"""
        [out:json][timeout:25];
        (
          node["leisure"="dog_park"](around:{radius_m},{lat},{lng});
          way["leisure"="dog_park"](around:{radius_m},{lat},{lng});
        );
        out center;
        """

        elements = await self._query_overpass(query)
        dog_parks = []

        for el in elements:
            center = el.get("center", {})
            el_lat = center.get("lat") or el.get("lat")
            el_lng = center.get("lon") or el.get("lon")

            if el_lat and el_lng:
                distance = haversine_distance(lat, lng, el_lat, el_lng)
                dog_parks.append(
                    {
                        "name": el.get("tags", {}).get("name", "Unnamed Dog Park"),
                        "distance_m": int(distance),
                        "lat": el_lat,
                        "lng": el_lng,
                    }
                )

        return sorted(dog_parks, key=lambda x: x["distance_m"])

    async def enrich(self, lat: float, lng: float) -> AmenityData:
        """Get all amenity data for a location."""
        # Run queries sequentially to respect Overpass API rate limits
        parks = await self.get_nearby_parks(lat, lng)
        cafes = await self.get_nearby_coffee_shops(lat, lng)
        dog_parks = await self.get_nearby_dog_parks(lat, lng)

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

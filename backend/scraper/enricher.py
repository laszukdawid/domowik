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
        self.client = httpx.AsyncClient(timeout=10.0)  # Reduced timeout for local

    async def close(self):
        await self.client.aclose()

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

    def _extract_geometry(
        self, el: dict
    ) -> tuple[dict | None, float | None, float | None]:
        """
        Extract geometry and centroid from an Overpass element.

        Returns:
            Tuple of (geometry_dict, centroid_lat, centroid_lng)
        """
        el_type = el.get("type")
        geometry = None
        centroid_lat, centroid_lng = None, None

        if el_type == "way" and "geometry" in el:
            # Way with geometry - build polygon coordinates
            coords = [[pt["lon"], pt["lat"]] for pt in el["geometry"]]
            if coords and coords[0] != coords[-1]:
                coords.append(coords[0])  # Close polygon
            geometry = {"type": "Polygon", "coordinates": [coords]}
            # Calculate centroid
            lats = [pt["lat"] for pt in el["geometry"]]
            lngs = [pt["lon"] for pt in el["geometry"]]
            centroid_lat = sum(lats) / len(lats)
            centroid_lng = sum(lngs) / len(lngs)
        elif el_type == "relation" and "bounds" in el:
            # Relation - use bounds center as approximation
            bounds = el["bounds"]
            centroid_lat = (bounds["minlat"] + bounds["maxlat"]) / 2
            centroid_lng = (bounds["minlon"] + bounds["maxlon"]) / 2
            geometry = {"type": "Point", "coordinates": [centroid_lng, centroid_lat]}
        elif el_type == "node":
            centroid_lat, centroid_lng = el.get("lat"), el.get("lon")
            if centroid_lat and centroid_lng:
                geometry = {"type": "Point", "coordinates": [centroid_lng, centroid_lat]}
        elif "center" in el:
            center = el["center"]
            centroid_lat, centroid_lng = center["lat"], center["lon"]
            geometry = {"type": "Point", "coordinates": [centroid_lng, centroid_lat]}

        return geometry, centroid_lat, centroid_lng

    async def get_nearby_parks(
        self, lat: float, lng: float, radius_m: int = 1000
    ) -> list[dict]:
        """Get parks within radius with full geometry."""
        query = f"""
        [out:json][timeout:10];
        (
          way["leisure"="park"](around:{radius_m},{lat},{lng});
          relation["leisure"="park"](around:{radius_m},{lat},{lng});
          way["leisure"="garden"](around:{radius_m},{lat},{lng});
          way["leisure"="playground"](around:{radius_m},{lat},{lng});
        );
        out body geom;
        """

        elements = await self._query_overpass(query)
        parks = []

        for el in elements:
            osm_id = el.get("id")
            geometry, centroid_lat, centroid_lng = self._extract_geometry(el)

            if centroid_lat and centroid_lng and osm_id and geometry:
                distance = haversine_distance(lat, lng, centroid_lat, centroid_lng)
                leisure_type = el.get("tags", {}).get("leisure", "park")
                parks.append({
                    "osm_id": osm_id,
                    "name": el.get("tags", {}).get("name", f"Unnamed {leisure_type.title()}"),
                    "type": leisure_type,  # park, garden, playground
                    "distance_m": int(distance),
                    "geometry": geometry,
                    "centroid_lat": centroid_lat,
                    "centroid_lng": centroid_lng,
                })

        return sorted(parks, key=lambda x: x["distance_m"])

    async def get_nearby_coffee_shops(
        self, lat: float, lng: float, radius_m: int = 1000
    ) -> list[dict]:
        """Get coffee shops within radius."""
        query = f"""
        [out:json][timeout:5];
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
            osm_id = el.get("id")
            geometry, centroid_lat, centroid_lng = self._extract_geometry(el)

            if centroid_lat and centroid_lng and osm_id and geometry:
                distance = haversine_distance(lat, lng, centroid_lat, centroid_lng)
                cafes.append({
                    "osm_id": osm_id,
                    "name": el.get("tags", {}).get("name", "Unnamed Cafe"),
                    "type": "coffee_shop",
                    "distance_m": int(distance),
                    "geometry": geometry,
                    "centroid_lat": centroid_lat,
                    "centroid_lng": centroid_lng,
                })

        return sorted(cafes, key=lambda x: x["distance_m"])

    async def get_nearby_dog_parks(
        self, lat: float, lng: float, radius_m: int = 2000
    ) -> list[dict]:
        """Get dog parks within radius."""
        query = f"""
        [out:json][timeout:5];
        (
          node["leisure"="dog_park"](around:{radius_m},{lat},{lng});
          way["leisure"="dog_park"](around:{radius_m},{lat},{lng});
        );
        out body geom;
        """

        elements = await self._query_overpass(query)
        dog_parks = []

        for el in elements:
            osm_id = el.get("id")
            geometry, centroid_lat, centroid_lng = self._extract_geometry(el)

            if centroid_lat and centroid_lng and osm_id and geometry:
                distance = haversine_distance(lat, lng, centroid_lat, centroid_lng)
                dog_parks.append({
                    "osm_id": osm_id,
                    "name": el.get("tags", {}).get("name", "Unnamed Dog Park"),
                    "type": "dog_park",
                    "distance_m": int(distance),
                    "geometry": geometry,
                    "centroid_lat": centroid_lat,
                    "centroid_lng": centroid_lng,
                })

        return sorted(dog_parks, key=lambda x: x["distance_m"])

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

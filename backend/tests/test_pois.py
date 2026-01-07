"""
Tests for POI (Point of Interest) functionality.

Tests geometry extraction, POI data structure, and enricher POI output.
"""

import pytest
import respx
from httpx import Response

from scraper.enricher import AmenityEnricher
from app.config import settings


class TestEnricherPOIGeometry:
    """Tests for POI geometry extraction in the enricher."""

    @respx.mock
    async def test_parks_include_osm_id(self, overpass_parks_response):
        """Parks should include OSM ID for deduplication."""
        respx.post(settings.overpass_url).mock(
            return_value=Response(200, json=overpass_parks_response)
        )

        enricher = AmenityEnricher()
        try:
            parks = await enricher.get_nearby_parks(49.2827, -123.1207)

            assert len(parks) > 0
            assert "osm_id" in parks[0]
            assert parks[0]["osm_id"] == 123456
        finally:
            await enricher.close()

    @respx.mock
    async def test_parks_include_geometry(self, overpass_parks_response):
        """Parks should include GeoJSON geometry."""
        respx.post(settings.overpass_url).mock(
            return_value=Response(200, json=overpass_parks_response)
        )

        enricher = AmenityEnricher()
        try:
            parks = await enricher.get_nearby_parks(49.2827, -123.1207)

            assert len(parks) > 0
            assert "geometry" in parks[0]
            assert parks[0]["geometry"]["type"] == "Polygon"
            assert "coordinates" in parks[0]["geometry"]
        finally:
            await enricher.close()

    @respx.mock
    async def test_parks_include_type(self, overpass_parks_response):
        """Parks should include their leisure type (park, garden, playground)."""
        respx.post(settings.overpass_url).mock(
            return_value=Response(200, json=overpass_parks_response)
        )

        enricher = AmenityEnricher()
        try:
            parks = await enricher.get_nearby_parks(49.2827, -123.1207)

            assert len(parks) > 0
            assert "type" in parks[0]
            assert parks[0]["type"] == "park"
        finally:
            await enricher.close()

    @respx.mock
    async def test_coffee_shops_include_geometry(self, overpass_coffee_shops_response):
        """Coffee shops should include point geometry."""
        respx.post(settings.overpass_url).mock(
            return_value=Response(200, json=overpass_coffee_shops_response)
        )

        enricher = AmenityEnricher()
        try:
            cafes = await enricher.get_nearby_coffee_shops(49.2827, -123.1207)

            assert len(cafes) > 0
            assert "geometry" in cafes[0]
            assert cafes[0]["geometry"]["type"] == "Point"
            assert "osm_id" in cafes[0]
            assert cafes[0]["type"] == "coffee_shop"
        finally:
            await enricher.close()

    @respx.mock
    async def test_dog_parks_include_geometry(self, overpass_dog_parks_response):
        """Dog parks should include geometry."""
        respx.post(settings.overpass_url).mock(
            return_value=Response(200, json=overpass_dog_parks_response)
        )

        enricher = AmenityEnricher()
        try:
            dog_parks = await enricher.get_nearby_dog_parks(49.2827, -123.1207)

            assert len(dog_parks) > 0
            assert "geometry" in dog_parks[0]
            assert "osm_id" in dog_parks[0]
            assert dog_parks[0]["type"] == "dog_park"
        finally:
            await enricher.close()


class TestEnricherGeometryExtraction:
    """Tests for the _extract_geometry helper method."""

    def test_extracts_polygon_from_way_with_geometry(self):
        """Should build polygon from way geometry points."""
        enricher = AmenityEnricher()

        element = {
            "type": "way",
            "id": 123,
            "geometry": [
                {"lat": 49.28, "lon": -123.12},
                {"lat": 49.29, "lon": -123.11},
                {"lat": 49.27, "lon": -123.10},
            ],
        }

        geometry, lat, lng = enricher._extract_geometry(element)

        assert geometry is not None
        assert geometry["type"] == "Polygon"
        assert len(geometry["coordinates"][0]) == 4  # Closed polygon
        assert lat is not None
        assert lng is not None

    def test_extracts_point_from_node(self):
        """Should extract point geometry from node elements."""
        enricher = AmenityEnricher()

        element = {
            "type": "node",
            "id": 456,
            "lat": 49.28,
            "lon": -123.12,
        }

        geometry, lat, lng = enricher._extract_geometry(element)

        assert geometry is not None
        assert geometry["type"] == "Point"
        assert geometry["coordinates"] == [-123.12, 49.28]
        assert lat == 49.28
        assert lng == -123.12

    def test_extracts_point_from_center(self):
        """Should extract point from center property when geometry not available."""
        enricher = AmenityEnricher()

        element = {
            "type": "way",
            "id": 789,
            "center": {"lat": 49.28, "lon": -123.12},
        }

        geometry, lat, lng = enricher._extract_geometry(element)

        assert geometry is not None
        assert geometry["type"] == "Point"
        assert lat == 49.28
        assert lng == -123.12

    def test_extracts_point_from_relation_bounds(self):
        """Should extract center point from relation bounds."""
        enricher = AmenityEnricher()

        element = {
            "type": "relation",
            "id": 101112,
            "bounds": {
                "minlat": 49.27,
                "maxlat": 49.29,
                "minlon": -123.13,
                "maxlon": -123.11,
            },
        }

        geometry, lat, lng = enricher._extract_geometry(element)

        assert geometry is not None
        assert geometry["type"] == "Point"
        assert lat == 49.28  # Center of bounds
        assert lng == -123.12

    def test_returns_none_for_invalid_element(self):
        """Should return None values for elements without coordinates."""
        enricher = AmenityEnricher()

        element = {
            "type": "way",
            "id": 999,
            # No geometry, center, or bounds
        }

        geometry, lat, lng = enricher._extract_geometry(element)

        assert geometry is None
        assert lat is None
        assert lng is None


class TestEnrichedDataStructure:
    """Tests that enriched data has correct structure for POI storage."""

    @respx.mock
    async def test_enrich_returns_all_pois_with_required_fields(
        self,
        overpass_parks_response,
        overpass_coffee_shops_response,
        overpass_dog_parks_response,
    ):
        """Enriched data should have all fields needed for POI upsert."""
        route = respx.post(settings.overpass_url)
        route.side_effect = [
            Response(200, json=overpass_parks_response),
            Response(200, json=overpass_coffee_shops_response),
            Response(200, json=overpass_dog_parks_response),
        ]

        enricher = AmenityEnricher()
        try:
            data = await enricher.enrich(49.2827, -123.1207)

            # Check parks have required fields
            for park in data.parks:
                assert "osm_id" in park
                assert "name" in park
                assert "type" in park
                assert "geometry" in park
                assert "distance_m" in park

            # Check coffee shops have required fields
            for cafe in data.coffee_shops:
                assert "osm_id" in cafe
                assert "name" in cafe
                assert "type" in cafe
                assert "geometry" in cafe
                assert "distance_m" in cafe

            # Check dog parks have required fields
            for dog_park in data.dog_parks:
                assert "osm_id" in dog_park
                assert "name" in dog_park
                assert "type" in dog_park
                assert "geometry" in dog_park
                assert "distance_m" in dog_park
        finally:
            await enricher.close()

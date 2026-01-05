"""
Tests for amenity enrichment logic.

Tests haversine distance calculation, walkability scoring, and Overpass API parsing.
"""

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


class TestHaversineDistance:
    """Tests for the haversine distance formula."""

    def test_same_point_returns_zero(self):
        """Distance from a point to itself should be 0."""
        distance = haversine_distance(49.2827, -123.1207, 49.2827, -123.1207)
        assert distance == 0

    def test_known_distance_vancouver_to_burnaby(self):
        """Distance from Vancouver downtown to Metrotown should be ~8km."""
        # Vancouver downtown (approx)
        lat1, lon1 = 49.2827, -123.1207
        # Metrotown Burnaby (approx)
        lat2, lon2 = 49.2276, -123.0076

        distance = haversine_distance(lat1, lon1, lat2, lon2)

        # Should be roughly 10km (allowing 20% tolerance for approximation)
        assert 8000 < distance < 12000

    def test_short_distance_within_neighborhood(self):
        """A short walk (~200m) should calculate correctly."""
        # Two points roughly 200m apart in Vancouver
        lat1, lon1 = 49.2827, -123.1207
        lat2, lon2 = 49.2845, -123.1207  # ~200m north

        distance = haversine_distance(lat1, lon1, lat2, lon2)

        # Should be roughly 200m (allowing some tolerance)
        assert 150 < distance < 250

    def test_returns_float(self):
        """Distance should be returned as a float."""
        distance = haversine_distance(49.0, -123.0, 49.1, -123.1)
        assert isinstance(distance, float)


class TestWalkabilityScoreParks:
    """Tests for park contribution to walkability score."""

    def test_park_within_200m_adds_20_points(self):
        """Park within 200m should add maximum 20 points."""
        data = AmenityData(nearest_park_m=150, parks=[{"distance_m": 150}])
        score = calculate_walkability_score(data)

        # 20 (park distance) + 2 (1 park within 1km) = 22
        assert score >= 20

    def test_park_within_500m_adds_15_points(self):
        """Park between 200-500m should add 15 points."""
        data = AmenityData(nearest_park_m=400, parks=[{"distance_m": 400}])
        score = calculate_walkability_score(data)

        # 15 (park distance) + 2 (1 park within 1km) = 17
        assert score >= 15

    def test_park_within_1000m_adds_10_points(self):
        """Park between 500-1000m should add 10 points."""
        data = AmenityData(nearest_park_m=800, parks=[{"distance_m": 800}])
        score = calculate_walkability_score(data)

        # 10 (park distance) + 2 (1 park within 1km) = 12
        assert score >= 10

    def test_multiple_parks_add_bonus_points(self):
        """Multiple parks within 1km should add bonus points (2 per park, max 10)."""
        data = AmenityData(
            nearest_park_m=200,
            parks=[
                {"distance_m": 200},
                {"distance_m": 500},
                {"distance_m": 800},
                {"distance_m": 900},
                {"distance_m": 950},
            ],
        )
        score = calculate_walkability_score(data)

        # 20 (nearest park) + 10 (5 parks * 2, capped at 10) = 30
        assert score >= 30


class TestWalkabilityScoreCoffeeShops:
    """Tests for coffee shop contribution to walkability score."""

    def test_coffee_within_150m_adds_15_points(self):
        """Coffee shop within 150m should add maximum 15 points."""
        data = AmenityData(nearest_coffee_m=100, coffee_shops=[{"distance_m": 100}])
        score = calculate_walkability_score(data)

        assert score >= 15

    def test_coffee_within_300m_adds_12_points(self):
        """Coffee shop between 150-300m should add 12 points."""
        data = AmenityData(nearest_coffee_m=250, coffee_shops=[{"distance_m": 250}])
        score = calculate_walkability_score(data)

        assert score >= 12

    def test_coffee_within_500m_adds_8_points(self):
        """Coffee shop between 300-500m should add 8 points."""
        data = AmenityData(nearest_coffee_m=400, coffee_shops=[{"distance_m": 400}])
        score = calculate_walkability_score(data)

        assert score >= 8

    def test_multiple_coffee_shops_add_bonus_points(self):
        """Multiple coffee shops within 500m should add bonus points."""
        data = AmenityData(
            nearest_coffee_m=100,
            coffee_shops=[
                {"distance_m": 100},
                {"distance_m": 200},
                {"distance_m": 300},
                {"distance_m": 400},
                {"distance_m": 450},
            ],
        )
        score = calculate_walkability_score(data)

        # 15 (nearest coffee) + 10 (5 cafes * 2, capped at 10) = 25
        assert score >= 25


class TestWalkabilityScoreDogParks:
    """Tests for dog park contribution to walkability score."""

    def test_dog_park_within_500m_adds_15_points(self):
        """Dog park within 500m should add 15 points."""
        data = AmenityData(nearest_dog_park_m=400)
        score = calculate_walkability_score(data)

        assert score >= 15

    def test_dog_park_within_1000m_adds_10_points(self):
        """Dog park between 500-1000m should add 10 points."""
        data = AmenityData(nearest_dog_park_m=800)
        score = calculate_walkability_score(data)

        assert score >= 10

    def test_dog_park_within_2000m_adds_5_points(self):
        """Dog park between 1000-2000m should add 5 points."""
        data = AmenityData(nearest_dog_park_m=1500)
        score = calculate_walkability_score(data)

        assert score >= 5


class TestWalkabilityScoreCombinedBonus:
    """Tests for combined amenity bonus."""

    def test_all_three_amenities_nearby_adds_20_bonus(self):
        """Having park, coffee, and dog park nearby should add 20 bonus points."""
        data = AmenityData(
            nearest_park_m=500,  # within 1000m
            nearest_coffee_m=300,  # within 500m
            nearest_dog_park_m=1500,  # within 2000m
            parks=[{"distance_m": 500}],
            coffee_shops=[{"distance_m": 300}],
        )
        score = calculate_walkability_score(data)

        # Should include 20 point bonus for having all three
        # 15 (park 500m) + 2 (1 park) + 12 (coffee 300m) + 2 (1 cafe) + 5 (dog park 1500m) + 20 (all three) = 56
        assert score >= 50

    def test_park_and_coffee_adds_10_bonus(self):
        """Having park and coffee (but no dog park) should add 10 bonus points."""
        data = AmenityData(
            nearest_park_m=500,
            nearest_coffee_m=300,
            parks=[{"distance_m": 500}],
            coffee_shops=[{"distance_m": 300}],
        )
        score = calculate_walkability_score(data)

        # Should include 10 point bonus
        assert score >= 30

    def test_only_park_adds_5_bonus(self):
        """Having only park (no coffee or dog park) should add 5 bonus points."""
        data = AmenityData(
            nearest_park_m=500,
            parks=[{"distance_m": 500}],
        )
        score = calculate_walkability_score(data)

        # 15 (park) + 2 (1 park bonus) + 5 (has park) = 22
        assert score >= 20


class TestWalkabilityScoreCapping:
    """Tests that walkability score is capped at 100."""

    def test_score_never_exceeds_100(self):
        """Even with many amenities, score should cap at 100."""
        data = AmenityData(
            nearest_park_m=100,
            nearest_coffee_m=50,
            nearest_dog_park_m=200,
            parks=[{"distance_m": i * 100} for i in range(1, 11)],  # 10 parks
            coffee_shops=[{"distance_m": i * 50} for i in range(1, 11)],  # 10 cafes
        )
        score = calculate_walkability_score(data)

        assert score <= 100


class TestWalkabilityScoreNoAmenities:
    """Tests for locations with no amenities."""

    def test_no_amenities_returns_zero(self):
        """Location with no amenities should score 0."""
        data = AmenityData()
        score = calculate_walkability_score(data)

        assert score == 0


class TestOverpassParksParsing:
    """Tests for parsing parks from Overpass API response."""

    @respx.mock
    async def test_parses_parks_with_center_coordinates(self, overpass_parks_response):
        """Should extract parks with center coordinates (for ways/relations)."""
        respx.post(settings.overpass_url).mock(
            return_value=Response(200, json=overpass_parks_response)
        )

        enricher = AmenityEnricher()
        try:
            parks = await enricher.get_nearby_parks(49.2827, -123.1207)

            assert len(parks) == 2
            assert parks[0]["name"] == "Stanley Park"
            assert "distance_m" in parks[0]
            assert isinstance(parks[0]["distance_m"], int)
        finally:
            await enricher.close()

    @respx.mock
    async def test_sorts_parks_by_distance(self, overpass_parks_response):
        """Parks should be sorted by distance, nearest first."""
        respx.post(settings.overpass_url).mock(
            return_value=Response(200, json=overpass_parks_response)
        )

        enricher = AmenityEnricher()
        try:
            parks = await enricher.get_nearby_parks(49.2827, -123.1207)

            if len(parks) > 1:
                assert parks[0]["distance_m"] <= parks[1]["distance_m"]
        finally:
            await enricher.close()

    @respx.mock
    async def test_handles_unnamed_parks(self):
        """Parks without names should default to 'Unnamed Park'."""
        response = {
            "elements": [
                {
                    "type": "way",
                    "id": 111,
                    "center": {"lat": 49.2830, "lon": -123.1200},
                    "tags": {"leisure": "park"},  # No name tag
                }
            ]
        }
        respx.post(settings.overpass_url).mock(
            return_value=Response(200, json=response)
        )

        enricher = AmenityEnricher()
        try:
            parks = await enricher.get_nearby_parks(49.2827, -123.1207)

            assert len(parks) == 1
            assert parks[0]["name"] == "Unnamed Park"
        finally:
            await enricher.close()


class TestOverpassCoffeeShopsParsing:
    """Tests for parsing coffee shops from Overpass API response."""

    @respx.mock
    async def test_parses_coffee_shops_with_node_coordinates(
        self, overpass_coffee_shops_response
    ):
        """Should extract coffee shops using direct lat/lon (for nodes)."""
        respx.post(settings.overpass_url).mock(
            return_value=Response(200, json=overpass_coffee_shops_response)
        )

        enricher = AmenityEnricher()
        try:
            cafes = await enricher.get_nearby_coffee_shops(49.2827, -123.1207)

            assert len(cafes) == 2
            assert cafes[0]["name"] == "49th Parallel"
            assert "distance_m" in cafes[0]
        finally:
            await enricher.close()


class TestOverpassDogParksParsing:
    """Tests for parsing dog parks from Overpass API response."""

    @respx.mock
    async def test_parses_dog_parks(self, overpass_dog_parks_response):
        """Should extract dog parks from response."""
        respx.post(settings.overpass_url).mock(
            return_value=Response(200, json=overpass_dog_parks_response)
        )

        enricher = AmenityEnricher()
        try:
            dog_parks = await enricher.get_nearby_dog_parks(49.2827, -123.1207)

            assert len(dog_parks) == 1
            assert dog_parks[0]["name"] == "Canine Commons"
        finally:
            await enricher.close()


class TestOverpassEmptyResponse:
    """Tests for handling empty Overpass API responses."""

    @respx.mock
    async def test_returns_empty_list_when_no_results(self, overpass_empty_response):
        """Should return empty list when no amenities found."""
        respx.post(settings.overpass_url).mock(
            return_value=Response(200, json=overpass_empty_response)
        )

        enricher = AmenityEnricher()
        try:
            parks = await enricher.get_nearby_parks(49.2827, -123.1207)
            assert parks == []
        finally:
            await enricher.close()


class TestOverpassErrorHandling:
    """Tests for Overpass API error handling."""

    @respx.mock
    async def test_returns_empty_list_on_http_error(self):
        """Should return empty list when Overpass API fails."""
        respx.post(settings.overpass_url).mock(
            return_value=Response(503)
        )

        enricher = AmenityEnricher()
        try:
            parks = await enricher.get_nearby_parks(49.2827, -123.1207)
            assert parks == []
        finally:
            await enricher.close()


class TestEnrichCombined:
    """Tests for the combined enrich() method."""

    @respx.mock
    async def test_enriches_location_with_all_amenity_types(
        self,
        overpass_parks_response,
        overpass_coffee_shops_response,
        overpass_dog_parks_response,
    ):
        """Should query all amenity types and compute scores."""
        # Mock all three queries (they'll all hit the same endpoint)
        route = respx.post(settings.overpass_url)
        route.side_effect = [
            Response(200, json=overpass_parks_response),
            Response(200, json=overpass_coffee_shops_response),
            Response(200, json=overpass_dog_parks_response),
        ]

        enricher = AmenityEnricher()
        try:
            data = await enricher.enrich(49.2827, -123.1207)

            assert data.nearest_park_m is not None
            assert data.nearest_coffee_m is not None
            assert data.nearest_dog_park_m is not None
            assert len(data.parks) > 0
            assert len(data.coffee_shops) > 0
            assert len(data.dog_parks) > 0
            assert data.walkability_score > 0
            assert data.amenity_score == data.walkability_score
        finally:
            await enricher.close()

    @respx.mock
    async def test_limits_amenities_to_reasonable_count(
        self,
        overpass_parks_response,
        overpass_coffee_shops_response,
        overpass_dog_parks_response,
    ):
        """Should limit stored amenities (10 parks, 10 cafes, 5 dog parks)."""
        # Create response with many parks
        many_parks = {
            "elements": [
                {
                    "type": "way",
                    "id": i,
                    "center": {"lat": 49.2827 + i * 0.001, "lon": -123.1207},
                    "tags": {"name": f"Park {i}", "leisure": "park"},
                }
                for i in range(20)
            ]
        }

        route = respx.post(settings.overpass_url)
        route.side_effect = [
            Response(200, json=many_parks),
            Response(200, json=overpass_coffee_shops_response),
            Response(200, json=overpass_dog_parks_response),
        ]

        enricher = AmenityEnricher()
        try:
            data = await enricher.enrich(49.2827, -123.1207)

            assert len(data.parks) <= 10
            assert len(data.coffee_shops) <= 10
            assert len(data.dog_parks) <= 5
        finally:
            await enricher.close()


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

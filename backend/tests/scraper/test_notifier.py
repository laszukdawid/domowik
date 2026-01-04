"""
Tests for the notification system.

Tests the preference matching logic that determines which listings to send to users.
"""

import pytest
from unittest.mock import MagicMock

from scraper.notifier import matches_preferences


def make_listing(
    price: int = 1000000,
    bedrooms: int | None = 3,
    sqft: int | None = 1500,
    city: str = "Vancouver",
    property_type: str | None = "House",
    nearest_park_m: int | None = None,
) -> MagicMock:
    """Create a mock Listing object for testing."""
    listing = MagicMock()
    listing.price = price
    listing.bedrooms = bedrooms
    listing.sqft = sqft
    listing.city = city
    listing.property_type = property_type

    if nearest_park_m is not None:
        listing.amenity_score = MagicMock()
        listing.amenity_score.nearest_park_m = nearest_park_m
    else:
        listing.amenity_score = None

    return listing


def make_preferences(
    min_price: int | None = None,
    max_price: int | None = None,
    min_bedrooms: int | None = None,
    min_sqft: int | None = None,
    cities: list[str] | None = None,
    property_types: list[str] | None = None,
    max_park_distance: int | None = None,
) -> MagicMock:
    """Create a mock UserPreferences object for testing."""
    prefs = MagicMock()
    prefs.min_price = min_price
    prefs.max_price = max_price
    prefs.min_bedrooms = min_bedrooms
    prefs.min_sqft = min_sqft
    prefs.cities = cities
    prefs.property_types = property_types
    prefs.max_park_distance = max_park_distance
    return prefs


class TestPriceFiltering:
    """Tests for price range filtering."""

    def test_matches_when_price_within_range(self):
        """Listing should match when price is within min/max range."""
        listing = make_listing(price=500000)
        prefs = make_preferences(min_price=400000, max_price=600000)

        assert matches_preferences(listing, prefs) is True

    def test_rejects_when_price_below_minimum(self):
        """Listing should be rejected when price is below minimum."""
        listing = make_listing(price=300000)
        prefs = make_preferences(min_price=400000)

        assert matches_preferences(listing, prefs) is False

    def test_rejects_when_price_above_maximum(self):
        """Listing should be rejected when price exceeds maximum."""
        listing = make_listing(price=700000)
        prefs = make_preferences(max_price=600000)

        assert matches_preferences(listing, prefs) is False

    def test_matches_when_no_price_constraints(self):
        """Listing should match when no price constraints are set."""
        listing = make_listing(price=999999999)
        prefs = make_preferences()

        assert matches_preferences(listing, prefs) is True

    def test_matches_at_exact_minimum_price(self):
        """Listing should match when price equals minimum (boundary)."""
        listing = make_listing(price=400000)
        prefs = make_preferences(min_price=400000)

        assert matches_preferences(listing, prefs) is True

    def test_matches_at_exact_maximum_price(self):
        """Listing should match when price equals maximum (boundary)."""
        listing = make_listing(price=600000)
        prefs = make_preferences(max_price=600000)

        assert matches_preferences(listing, prefs) is True


class TestBedroomFiltering:
    """Tests for bedroom count filtering."""

    def test_matches_when_bedrooms_meet_minimum(self):
        """Listing should match when bedrooms >= minimum."""
        listing = make_listing(bedrooms=4)
        prefs = make_preferences(min_bedrooms=3)

        assert matches_preferences(listing, prefs) is True

    def test_rejects_when_bedrooms_below_minimum(self):
        """Listing should be rejected when bedrooms < minimum."""
        listing = make_listing(bedrooms=2)
        prefs = make_preferences(min_bedrooms=3)

        assert matches_preferences(listing, prefs) is False

    def test_rejects_when_bedrooms_is_none_and_minimum_set(self):
        """Listing with no bedroom info should be rejected when minimum is set."""
        listing = make_listing(bedrooms=None)
        prefs = make_preferences(min_bedrooms=2)

        assert matches_preferences(listing, prefs) is False

    def test_matches_when_no_bedroom_constraints(self):
        """Listing should match when no bedroom constraint is set."""
        listing = make_listing(bedrooms=1)
        prefs = make_preferences()

        assert matches_preferences(listing, prefs) is True

    def test_matches_at_exact_minimum_bedrooms(self):
        """Listing should match when bedrooms equals minimum (boundary)."""
        listing = make_listing(bedrooms=3)
        prefs = make_preferences(min_bedrooms=3)

        assert matches_preferences(listing, prefs) is True


class TestSqftFiltering:
    """Tests for square footage filtering."""

    def test_matches_when_sqft_meets_minimum(self):
        """Listing should match when sqft >= minimum."""
        listing = make_listing(sqft=2000)
        prefs = make_preferences(min_sqft=1500)

        assert matches_preferences(listing, prefs) is True

    def test_rejects_when_sqft_below_minimum(self):
        """Listing should be rejected when sqft < minimum."""
        listing = make_listing(sqft=1000)
        prefs = make_preferences(min_sqft=1500)

        assert matches_preferences(listing, prefs) is False

    def test_rejects_when_sqft_is_none_and_minimum_set(self):
        """Listing with no sqft info should be rejected when minimum is set."""
        listing = make_listing(sqft=None)
        prefs = make_preferences(min_sqft=1000)

        assert matches_preferences(listing, prefs) is False

    def test_matches_when_no_sqft_constraints(self):
        """Listing should match when no sqft constraint is set."""
        listing = make_listing(sqft=500)
        prefs = make_preferences()

        assert matches_preferences(listing, prefs) is True


class TestCityFiltering:
    """Tests for city filtering."""

    def test_matches_when_city_in_list(self):
        """Listing should match when city is in the preference list."""
        listing = make_listing(city="Vancouver")
        prefs = make_preferences(cities=["Vancouver", "Burnaby", "Richmond"])

        assert matches_preferences(listing, prefs) is True

    def test_rejects_when_city_not_in_list(self):
        """Listing should be rejected when city is not in preference list."""
        listing = make_listing(city="Surrey")
        prefs = make_preferences(cities=["Vancouver", "Burnaby"])

        assert matches_preferences(listing, prefs) is False

    def test_matches_when_no_city_constraints(self):
        """Listing should match when no city constraint is set."""
        listing = make_listing(city="Anywhere")
        prefs = make_preferences(cities=None)

        assert matches_preferences(listing, prefs) is True

    def test_matches_when_cities_list_is_empty(self):
        """Listing should match when cities list is empty (no constraint)."""
        listing = make_listing(city="Vancouver")
        prefs = make_preferences(cities=[])

        # Empty list should be treated as "no constraint"
        assert matches_preferences(listing, prefs) is True


class TestPropertyTypeFiltering:
    """Tests for property type filtering."""

    def test_matches_when_property_type_in_list(self):
        """Listing should match when property type is in preference list."""
        listing = make_listing(property_type="House")
        prefs = make_preferences(property_types=["House", "Townhouse"])

        assert matches_preferences(listing, prefs) is True

    def test_rejects_when_property_type_not_in_list(self):
        """Listing should be rejected when property type not in preference list."""
        listing = make_listing(property_type="Condo")
        prefs = make_preferences(property_types=["House", "Townhouse"])

        assert matches_preferences(listing, prefs) is False

    def test_matches_when_no_property_type_constraints(self):
        """Listing should match when no property type constraint is set."""
        listing = make_listing(property_type="Condo")
        prefs = make_preferences(property_types=None)

        assert matches_preferences(listing, prefs) is True

    def test_rejects_when_property_type_is_none_but_constraint_set(self):
        """Listing with no property type should be rejected when constraint is set."""
        listing = make_listing(property_type=None)
        prefs = make_preferences(property_types=["House"])

        assert matches_preferences(listing, prefs) is False


class TestParkDistanceFiltering:
    """Tests for max park distance filtering."""

    def test_matches_when_park_within_max_distance(self):
        """Listing should match when nearest park is within max distance."""
        listing = make_listing(nearest_park_m=300)
        prefs = make_preferences(max_park_distance=500)

        assert matches_preferences(listing, prefs) is True

    def test_rejects_when_park_exceeds_max_distance(self):
        """Listing should be rejected when nearest park exceeds max distance."""
        listing = make_listing(nearest_park_m=800)
        prefs = make_preferences(max_park_distance=500)

        assert matches_preferences(listing, prefs) is False

    def test_rejects_when_no_park_data_but_constraint_set(self):
        """Listing with no park data should be rejected when constraint is set."""
        listing = make_listing(nearest_park_m=None)
        prefs = make_preferences(max_park_distance=500)

        # No amenity_score at all
        listing.amenity_score = None
        assert matches_preferences(listing, prefs) is False

    def test_rejects_when_park_distance_is_none_but_constraint_set(self):
        """Listing where nearest_park_m is None should be rejected."""
        listing = make_listing()
        listing.amenity_score = MagicMock()
        listing.amenity_score.nearest_park_m = None
        prefs = make_preferences(max_park_distance=500)

        assert matches_preferences(listing, prefs) is False

    def test_matches_when_no_park_distance_constraint(self):
        """Listing should match when no park distance constraint is set."""
        listing = make_listing(nearest_park_m=5000)
        prefs = make_preferences(max_park_distance=None)

        assert matches_preferences(listing, prefs) is True

    def test_matches_at_exact_max_distance(self):
        """Listing should match when park distance equals max (boundary)."""
        listing = make_listing(nearest_park_m=500)
        prefs = make_preferences(max_park_distance=500)

        assert matches_preferences(listing, prefs) is True


class TestCombinedFiltering:
    """Tests for multiple filters applied together."""

    def test_matches_when_all_criteria_met(self):
        """Listing should match when all preference criteria are satisfied."""
        listing = make_listing(
            price=800000,
            bedrooms=3,
            sqft=1800,
            city="Vancouver",
            property_type="House",
            nearest_park_m=200,
        )
        prefs = make_preferences(
            min_price=500000,
            max_price=1000000,
            min_bedrooms=3,
            min_sqft=1500,
            cities=["Vancouver", "Burnaby"],
            property_types=["House", "Townhouse"],
            max_park_distance=500,
        )

        assert matches_preferences(listing, prefs) is True

    def test_rejects_when_one_criterion_fails(self):
        """Listing should be rejected if any single criterion fails."""
        listing = make_listing(
            price=800000,
            bedrooms=2,  # This fails (need 3)
            sqft=1800,
            city="Vancouver",
            property_type="House",
        )
        prefs = make_preferences(
            min_price=500000,
            max_price=1000000,
            min_bedrooms=3,
            min_sqft=1500,
            cities=["Vancouver"],
            property_types=["House"],
        )

        assert matches_preferences(listing, prefs) is False

    def test_matches_with_no_constraints(self):
        """Any listing should match when no preferences are set."""
        listing = make_listing(
            price=100,
            bedrooms=0,
            sqft=100,
            city="Unknown",
            property_type="Shack",
        )
        prefs = make_preferences()

        assert matches_preferences(listing, prefs) is True

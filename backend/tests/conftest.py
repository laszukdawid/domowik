"""
Pytest configuration and shared fixtures.
"""

import pytest


@pytest.fixture
def realtor_ca_token_response() -> dict:
    """Mock response from realtor.ca token endpoint."""
    return {"token": "mock-reese84-token-abc123"}


@pytest.fixture
def realtor_ca_listing_standard() -> dict:
    """A standard realtor.ca listing with all fields populated."""
    return {
        "MlsNumber": "R2912345",
        "Property": {
            "Address": {
                "AddressText": "123 Main Street|Vancouver, British Columbia V6B1A1",
                "Latitude": 49.2827,
                "Longitude": -123.1207,
            },
            "Price": "$1,299,000",
        },
        "Building": {
            "Bedrooms": "3",
            "BathroomTotal": "2",
            "SizeInterior": "1,850 sqft",
            "Type": "House",
        },
        "PostedDate": "2026-01-01T10:00:00Z",
    }


@pytest.fixture
def realtor_ca_listing_minimal() -> dict:
    """A listing with minimal required fields only."""
    return {
        "MlsNumber": "R2900001",
        "Property": {
            "Address": {
                "AddressText": "456 Oak Ave|Burnaby, British Columbia V5H2N1",
                "Latitude": 49.2488,
                "Longitude": -123.0016,
            },
            "Price": "$899,000",
        },
        "Building": {},
    }


@pytest.fixture
def realtor_ca_listing_complex_bedrooms() -> dict:
    """A listing with '2 + 1' bedroom format."""
    return {
        "MlsNumber": "R2900002",
        "Property": {
            "Address": {
                "AddressText": "789 Pine St|Richmond, British Columbia V6Y1P3",
                "Latitude": 49.1666,
                "Longitude": -123.1336,
            },
            "Price": "$750,000",
        },
        "Building": {
            "Bedrooms": "2 + 1",
            "BathroomTotal": "1 + 1",
            "SizeInterior": "1200 sqft",
            "Type": "Townhouse",
        },
    }


@pytest.fixture
def realtor_ca_listing_no_pipe_address() -> dict:
    """A listing where AddressText doesn't have the expected pipe separator."""
    return {
        "MlsNumber": "R2900003",
        "Property": {
            "Address": {
                "AddressText": "999 Unknown Road",
                "Latitude": 49.2000,
                "Longitude": -123.0000,
            },
            "Price": "$500,000",
        },
        "Building": {},
    }


@pytest.fixture
def realtor_ca_search_response(realtor_ca_listing_standard, realtor_ca_listing_minimal) -> dict:
    """Mock search API response with pagination."""
    return {
        "Results": [realtor_ca_listing_standard, realtor_ca_listing_minimal],
        "Paging": {
            "TotalRecords": 2,
            "CurrentPage": 1,
            "RecordsPerPage": 200,
        },
    }


@pytest.fixture
def overpass_parks_response() -> dict:
    """Mock Overpass API response for parks query."""
    return {
        "elements": [
            {
                "type": "way",
                "id": 123456,
                "center": {"lat": 49.2830, "lon": -123.1200},
                "tags": {"name": "Stanley Park", "leisure": "park"},
            },
            {
                "type": "way",
                "id": 123457,
                "center": {"lat": 49.2850, "lon": -123.1250},
                "tags": {"name": "Victory Square", "leisure": "park"},
            },
        ]
    }


@pytest.fixture
def overpass_coffee_shops_response() -> dict:
    """Mock Overpass API response for coffee shops query."""
    return {
        "elements": [
            {
                "type": "node",
                "id": 789012,
                "lat": 49.2825,
                "lon": -123.1205,
                "tags": {"name": "49th Parallel", "amenity": "cafe"},
            },
            {
                "type": "node",
                "id": 789013,
                "lat": 49.2835,
                "lon": -123.1215,
                "tags": {"name": "Starbucks", "amenity": "cafe"},
            },
        ]
    }


@pytest.fixture
def overpass_dog_parks_response() -> dict:
    """Mock Overpass API response for dog parks query."""
    return {
        "elements": [
            {
                "type": "way",
                "id": 456789,
                "center": {"lat": 49.2900, "lon": -123.1100},
                "tags": {"name": "Canine Commons", "leisure": "dog_park"},
            },
        ]
    }


@pytest.fixture
def overpass_empty_response() -> dict:
    """Mock Overpass API response with no results."""
    return {"elements": []}


@pytest.fixture
def local_overpass_url() -> str:
    """Local Overpass API URL for testing."""
    return "http://localhost:12345/api/interpreter"

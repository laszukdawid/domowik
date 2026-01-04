"""
Tests for realtor.ca scraper.

Tests the parsing logic to ensure listings are correctly extracted from API responses.
"""

import pytest
import respx
from httpx import Response

from scraper.realtor_ca import RealtorCaScraper, ScrapedListing


class TestParseListingAddress:
    """Tests for address and city parsing from AddressText field."""

    def test_extracts_street_address_before_pipe(self, realtor_ca_listing_standard):
        """Address should be the part before the pipe separator."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_standard)

        assert listing is not None
        assert listing.address == "123 Main Street"

    def test_extracts_city_from_after_pipe(self, realtor_ca_listing_standard):
        """City should be extracted from the part after pipe, before comma."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_standard)

        assert listing is not None
        assert listing.city == "Vancouver"

    def test_handles_burnaby_city(self, realtor_ca_listing_minimal):
        """Should correctly parse Burnaby from address."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_minimal)

        assert listing is not None
        assert listing.city == "Burnaby"

    def test_handles_address_without_pipe_separator(self, realtor_ca_listing_no_pipe_address):
        """When no pipe in address, city should default to 'Unknown'."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_no_pipe_address)

        assert listing is not None
        assert listing.city == "Unknown"
        assert listing.address == "999 Unknown Road"


class TestParseListingPrice:
    """Tests for price parsing."""

    def test_parses_price_with_dollar_and_comma(self, realtor_ca_listing_standard):
        """Should extract numeric price from '$1,299,000' format."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_standard)

        assert listing is not None
        assert listing.price == 1299000

    def test_parses_price_without_comma(self, realtor_ca_listing_minimal):
        """Should handle prices like '$899,000'."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_minimal)

        assert listing is not None
        assert listing.price == 899000


class TestParseListingBedrooms:
    """Tests for bedroom parsing, including the '2 + 1' format."""

    def test_parses_simple_bedroom_count(self, realtor_ca_listing_standard):
        """Should parse '3' as 3 bedrooms."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_standard)

        assert listing is not None
        assert listing.bedrooms == 3

    def test_parses_plus_format_takes_first_number(self, realtor_ca_listing_complex_bedrooms):
        """Should parse '2 + 1' as 2 bedrooms (main floor only)."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_complex_bedrooms)

        assert listing is not None
        assert listing.bedrooms == 2

    def test_missing_bedrooms_returns_none(self, realtor_ca_listing_minimal):
        """Should return None when bedrooms not specified."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_minimal)

        assert listing is not None
        assert listing.bedrooms is None


class TestParseListingBathrooms:
    """Tests for bathroom parsing."""

    def test_parses_simple_bathroom_count(self, realtor_ca_listing_standard):
        """Should parse '2' as 2 bathrooms."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_standard)

        assert listing is not None
        assert listing.bathrooms == 2

    def test_parses_plus_format_takes_first_number(self, realtor_ca_listing_complex_bedrooms):
        """Should parse '1 + 1' as 1 bathroom."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_complex_bedrooms)

        assert listing is not None
        assert listing.bathrooms == 1

    def test_missing_bathrooms_returns_none(self, realtor_ca_listing_minimal):
        """Should return None when bathrooms not specified."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_minimal)

        assert listing is not None
        assert listing.bathrooms is None


class TestParseListingSqft:
    """Tests for square footage parsing."""

    def test_parses_sqft_with_comma(self, realtor_ca_listing_standard):
        """Should parse '1,850 sqft' as 1850."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_standard)

        assert listing is not None
        assert listing.sqft == 1850

    def test_parses_sqft_without_comma(self, realtor_ca_listing_complex_bedrooms):
        """Should parse '1200 sqft' as 1200."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_complex_bedrooms)

        assert listing is not None
        assert listing.sqft == 1200

    def test_missing_sqft_returns_none(self, realtor_ca_listing_minimal):
        """Should return None when sqft not specified."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_minimal)

        assert listing is not None
        assert listing.sqft is None


class TestParseListingCoordinates:
    """Tests for latitude/longitude parsing."""

    def test_extracts_coordinates(self, realtor_ca_listing_standard):
        """Should extract lat/lng from Property.Address."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_standard)

        assert listing is not None
        assert listing.latitude == 49.2827
        assert listing.longitude == -123.1207


class TestParseListingMetadata:
    """Tests for MLS ID, URL, property type, and date."""

    def test_extracts_mls_id(self, realtor_ca_listing_standard):
        """Should extract MlsNumber as mls_id."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_standard)

        assert listing is not None
        assert listing.mls_id == "R2912345"

    def test_generates_correct_url(self, realtor_ca_listing_standard):
        """Should generate realtor.ca URL from MLS number."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_standard)

        assert listing is not None
        assert listing.url == "https://www.realtor.ca/real-estate/R2912345"

    def test_extracts_property_type(self, realtor_ca_listing_standard):
        """Should extract Building.Type."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_standard)

        assert listing is not None
        assert listing.property_type == "House"

    def test_parses_listing_date(self, realtor_ca_listing_standard):
        """Should parse ISO date from PostedDate."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_standard)

        assert listing is not None
        assert listing.listing_date is not None
        assert listing.listing_date.year == 2026
        assert listing.listing_date.month == 1
        assert listing.listing_date.day == 1

    def test_stores_raw_data(self, realtor_ca_listing_standard):
        """Should store the original API response in raw_data."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing(realtor_ca_listing_standard)

        assert listing is not None
        assert listing.raw_data == realtor_ca_listing_standard


class TestParseListingEdgeCases:
    """Tests for edge cases and error handling."""

    def test_returns_none_for_missing_mls_number(self):
        """Should return None if MlsNumber is missing."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing({"Property": {}})

        assert listing is None

    def test_returns_none_for_empty_mls_number(self):
        """Should return None if MlsNumber is empty string."""
        scraper = RealtorCaScraper()
        listing = scraper._parse_listing({"MlsNumber": "", "Property": {}})

        assert listing is None


class TestFetchToken:
    """Tests for reese84 token fetching."""

    @respx.mock
    async def test_fetches_token_with_correct_payload(self, realtor_ca_token_response):
        """Should POST to token URL with correct payload structure."""
        token_route = respx.post(
            "https://www.realtor.ca/dnight-Exit-shall-Braith-Then-why-vponst-is-proc"
        ).mock(return_value=Response(200, json=realtor_ca_token_response))

        scraper = RealtorCaScraper()
        try:
            token = await scraper._fetch_reese84_token()

            assert token == "mock-reese84-token-abc123"
            assert token_route.called

            # Verify the request had correct structure
            request = token_route.calls[0].request
            assert request.url.params.get("d") == "www.realtor.ca"
        finally:
            await scraper.close()


class TestFetchPage:
    """Tests for fetching a page of listings."""

    @respx.mock
    async def test_fetches_page_and_parses_listings(
        self, realtor_ca_token_response, realtor_ca_search_response
    ):
        """Should fetch listings and return parsed results with total count."""
        respx.post(
            "https://www.realtor.ca/dnight-Exit-shall-Braith-Then-why-vponst-is-proc"
        ).mock(return_value=Response(200, json=realtor_ca_token_response))

        respx.post("https://api2.realtor.ca/Listing.svc/PropertySearch_Post").mock(
            return_value=Response(200, json=realtor_ca_search_response)
        )

        scraper = RealtorCaScraper()
        try:
            listings, total = await scraper.fetch_page(1)

            assert total == 2
            assert len(listings) == 2
            assert listings[0].mls_id == "R2912345"
            assert listings[1].mls_id == "R2900001"
        finally:
            await scraper.close()

    @respx.mock
    async def test_returns_empty_on_http_error(self, realtor_ca_token_response):
        """Should return empty list and 0 total on HTTP error."""
        respx.post(
            "https://www.realtor.ca/dnight-Exit-shall-Braith-Then-why-vponst-is-proc"
        ).mock(return_value=Response(200, json=realtor_ca_token_response))

        respx.post("https://api2.realtor.ca/Listing.svc/PropertySearch_Post").mock(
            return_value=Response(500)
        )

        scraper = RealtorCaScraper()
        try:
            listings, total = await scraper.fetch_page(1)

            assert listings == []
            assert total == 0
        finally:
            await scraper.close()


class TestSearchParams:
    """Tests for search parameter construction."""

    def test_includes_gva_bounds(self):
        """Search params should include Greater Vancouver Area bounds."""
        scraper = RealtorCaScraper()
        params = scraper._build_search_params(page=1)

        # GVA bounds from the code
        assert params["LatitudeMin"] == "49.0"
        assert params["LatitudeMax"] == "49.4"
        assert params["LongitudeMin"] == "-123.3"
        assert params["LongitudeMax"] == "-122.5"

    def test_includes_residential_property_type(self):
        """Should filter for residential properties."""
        scraper = RealtorCaScraper()
        params = scraper._build_search_params(page=1)

        assert params["PropertyTypeGroupID"] == "1"

    def test_includes_for_sale_transaction_type(self):
        """Should filter for properties for sale."""
        scraper = RealtorCaScraper()
        params = scraper._build_search_params(page=1)

        assert params["TransactionTypeId"] == "2"

    def test_includes_pagination(self):
        """Should include current page number."""
        scraper = RealtorCaScraper()

        params_page1 = scraper._build_search_params(page=1)
        params_page5 = scraper._build_search_params(page=5)

        assert params_page1["CurrentPage"] == "1"
        assert params_page5["CurrentPage"] == "5"

    def test_sorts_by_date_descending(self):
        """Should sort by listing date descending to get newest first."""
        scraper = RealtorCaScraper()
        params = scraper._build_search_params(page=1)

        assert params["Sort"] == "6-D"

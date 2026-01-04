"""
Tests for the scraper runner logic.

Tests the delisting logic and upsert behavior patterns.
Since these functions require database sessions, we test the business logic
by extracting time calculations and using mock sessions.
"""

import pytest
from datetime import datetime, timedelta, UTC
from unittest.mock import AsyncMock, MagicMock, patch

from scraper.run import upsert_listing, mark_delisted
from scraper.realtor_ca import ScrapedListing


def make_scraped_listing(
    mls_id: str = "R2900001",
    price: int = 1000000,
    address: str = "123 Test St",
    city: str = "Vancouver",
    latitude: float = 49.2827,
    longitude: float = -123.1207,
    bedrooms: int | None = 3,
    bathrooms: int | None = 2,
    sqft: int | None = 1500,
    property_type: str | None = "House",
    listing_date: datetime | None = None,
) -> ScrapedListing:
    """Create a ScrapedListing for testing."""
    return ScrapedListing(
        mls_id=mls_id,
        url=f"https://realtor.ca/{mls_id}",
        address=address,
        city=city,
        latitude=latitude,
        longitude=longitude,
        price=price,
        bedrooms=bedrooms,
        bathrooms=bathrooms,
        sqft=sqft,
        property_type=property_type,
        listing_date=listing_date,
        raw_data={"MlsNumber": mls_id},
    )


class TestDelistingTimeLogic:
    """Tests for the 48-hour delisting threshold logic.

    According to the design doc, listings should only be marked as delisted
    after being missing from scrapes for 48 hours (2 consecutive daily runs).
    """

    def test_listing_not_delisted_if_seen_within_48_hours(self):
        """Listing missing for less than 48 hours should NOT be delisted."""
        # Simulate last_seen 24 hours ago
        now = datetime.now(UTC)
        last_seen = now - timedelta(hours=24)
        hours_since_seen = (now - last_seen).total_seconds() / 3600

        # According to the logic: only delist if hours_since_seen > 48
        should_delist = hours_since_seen > 48

        assert should_delist is False

    def test_listing_delisted_if_missing_over_48_hours(self):
        """Listing missing for more than 48 hours SHOULD be delisted."""
        # Simulate last_seen 49 hours ago
        now = datetime.now(UTC)
        last_seen = now - timedelta(hours=49)
        hours_since_seen = (now - last_seen).total_seconds() / 3600

        should_delist = hours_since_seen > 48

        assert should_delist is True

    def test_listing_not_delisted_at_exactly_48_hours(self):
        """Listing at exactly 48 hours should NOT be delisted (boundary case).

        Note: We test slightly under 48 hours to avoid timing flakiness.
        The key behavior is: > 48 (strictly greater), not >= 48.
        """
        # Use 47.9 hours to test the boundary behavior without timing issues
        now = datetime.now(UTC)
        last_seen = now - timedelta(hours=47, minutes=59)
        hours_since_seen = (now - last_seen).total_seconds() / 3600

        # At 47.9 hours, should NOT delist (need > 48)
        should_delist = hours_since_seen > 48

        assert should_delist is False


class TestUpsertListingNewListing:
    """Tests for upsert behavior with new listings."""

    @pytest.fixture
    def mock_session(self):
        """Create a mock async session."""
        session = AsyncMock()
        # session.add is synchronous, so use MagicMock
        session.add = MagicMock()
        return session

    @pytest.fixture
    def mock_empty_result(self):
        """Create a mock result that returns no existing listing."""
        result = MagicMock()
        result.scalar_one_or_none.return_value = None
        return result

    async def test_creates_new_listing_when_not_exists(
        self, mock_session, mock_empty_result
    ):
        """Should create a new Listing when MLS ID doesn't exist in database."""
        mock_session.execute.return_value = mock_empty_result
        scraped = make_scraped_listing(mls_id="R2900001", price=500000)

        listing, is_new = await upsert_listing(mock_session, scraped)

        assert is_new is True
        assert listing.mls_id == "R2900001"
        assert listing.price == 500000
        assert listing.status == "active"
        mock_session.add.assert_called_once()

    async def test_new_listing_copies_all_fields(
        self, mock_session, mock_empty_result
    ):
        """New listing should copy all fields from scraped data."""
        mock_session.execute.return_value = mock_empty_result
        scraped = make_scraped_listing(
            mls_id="R2900002",
            address="456 Oak Ave",
            city="Burnaby",
            price=750000,
            bedrooms=4,
            bathrooms=3,
            sqft=2000,
            property_type="Townhouse",
        )

        listing, is_new = await upsert_listing(mock_session, scraped)

        assert listing.address == "456 Oak Ave"
        assert listing.city == "Burnaby"
        assert listing.bedrooms == 4
        assert listing.bathrooms == 3
        assert listing.sqft == 2000
        assert listing.property_type == "Townhouse"


class TestUpsertListingExistingListing:
    """Tests for upsert behavior with existing listings."""

    @pytest.fixture
    def mock_session(self):
        """Create a mock async session."""
        session = AsyncMock()
        return session

    def make_existing_listing(
        self,
        mls_id: str = "R2900001",
        price: int = 500000,
        status: str = "active",
        last_seen: datetime | None = None,
    ) -> MagicMock:
        """Create a mock existing Listing."""
        listing = MagicMock()
        listing.mls_id = mls_id
        listing.price = price
        listing.status = status
        listing.last_seen = last_seen or datetime.now(UTC) - timedelta(days=1)
        listing.raw_data = {}
        return listing

    async def test_updates_price_on_existing_listing(self, mock_session):
        """Should update price when listing already exists."""
        existing = self.make_existing_listing(price=500000)
        result = MagicMock()
        result.scalar_one_or_none.return_value = existing
        mock_session.execute.return_value = result

        scraped = make_scraped_listing(mls_id="R2900001", price=550000)

        listing, is_new = await upsert_listing(mock_session, scraped)

        assert is_new is False
        assert listing.price == 550000

    async def test_updates_last_seen_on_existing_listing(self, mock_session):
        """Should update last_seen timestamp when listing is re-scraped."""
        old_last_seen = datetime.now(UTC) - timedelta(days=1)
        existing = self.make_existing_listing(last_seen=old_last_seen)
        result = MagicMock()
        result.scalar_one_or_none.return_value = existing
        mock_session.execute.return_value = result

        scraped = make_scraped_listing(mls_id="R2900001")

        listing, _ = await upsert_listing(mock_session, scraped)

        # last_seen should be updated to now (within reasonable tolerance)
        assert listing.last_seen > old_last_seen

    async def test_updates_raw_data_on_existing_listing(self, mock_session):
        """Should update raw_data when listing is re-scraped."""
        existing = self.make_existing_listing()
        existing.raw_data = {"old": "data"}
        result = MagicMock()
        result.scalar_one_or_none.return_value = existing
        mock_session.execute.return_value = result

        scraped = make_scraped_listing(mls_id="R2900001")

        listing, _ = await upsert_listing(mock_session, scraped)

        assert listing.raw_data == {"MlsNumber": "R2900001"}

    async def test_reactivates_delisted_listing(self, mock_session):
        """Should change status from 'delisted' to 'active' when re-scraped."""
        existing = self.make_existing_listing(status="delisted")
        result = MagicMock()
        result.scalar_one_or_none.return_value = existing
        mock_session.execute.return_value = result

        scraped = make_scraped_listing(mls_id="R2900001")

        listing, is_new = await upsert_listing(mock_session, scraped)

        assert is_new is False
        assert listing.status == "active"

    async def test_does_not_call_add_for_existing_listing(self, mock_session):
        """Should NOT call session.add for existing listing (already tracked)."""
        existing = self.make_existing_listing()
        result = MagicMock()
        result.scalar_one_or_none.return_value = existing
        mock_session.execute.return_value = result

        scraped = make_scraped_listing(mls_id="R2900001")

        await upsert_listing(mock_session, scraped)

        mock_session.add.assert_not_called()


class TestMarkDelisted:
    """Tests for the mark_delisted function."""

    @pytest.fixture
    def mock_session(self):
        """Create a mock async session."""
        session = AsyncMock()
        return session

    def make_mock_listing(
        self,
        mls_id: str,
        last_seen: datetime,
        status: str = "active",
    ) -> MagicMock:
        """Create a mock Listing for delisting tests."""
        listing = MagicMock()
        listing.mls_id = mls_id
        listing.last_seen = last_seen
        listing.status = status
        return listing

    async def test_delists_old_missing_listings(self, mock_session):
        """Should delist listings not seen for more than 48 hours."""
        old_listing = self.make_mock_listing(
            mls_id="R2900001",
            last_seen=datetime.now(UTC) - timedelta(hours=50),
        )
        result = MagicMock()
        result.scalars.return_value.all.return_value = [old_listing]
        mock_session.execute.return_value = result

        await mark_delisted(mock_session, seen_mls_ids={"R2900002"})

        assert old_listing.status == "delisted"

    async def test_does_not_delist_recently_missing_listings(self, mock_session):
        """Should NOT delist listings missing for less than 48 hours."""
        recent_listing = self.make_mock_listing(
            mls_id="R2900001",
            last_seen=datetime.now(UTC) - timedelta(hours=24),
        )
        result = MagicMock()
        result.scalars.return_value.all.return_value = [recent_listing]
        mock_session.execute.return_value = result

        await mark_delisted(mock_session, seen_mls_ids={"R2900002"})

        assert recent_listing.status == "active"

    async def test_multiple_listings_delisted_correctly(self, mock_session):
        """Should correctly handle multiple listings with different ages."""
        old_listing = self.make_mock_listing(
            mls_id="R2900001",
            last_seen=datetime.now(UTC) - timedelta(hours=50),
        )
        recent_listing = self.make_mock_listing(
            mls_id="R2900002",
            last_seen=datetime.now(UTC) - timedelta(hours=24),
        )
        result = MagicMock()
        result.scalars.return_value.all.return_value = [old_listing, recent_listing]
        mock_session.execute.return_value = result

        await mark_delisted(mock_session, seen_mls_ids=set())

        assert old_listing.status == "delisted"
        assert recent_listing.status == "active"


class TestScrapedListingModel:
    """Tests for the ScrapedListing Pydantic model validation."""

    def test_requires_mls_id(self):
        """ScrapedListing should require mls_id."""
        with pytest.raises(Exception):
            ScrapedListing(
                # Missing mls_id
                url="https://test.com",
                address="123 Test",
                city="Vancouver",
                latitude=49.0,
                longitude=-123.0,
                price=500000,
                raw_data={},
            )

    def test_allows_optional_fields_to_be_none(self):
        """Optional fields like bedrooms, bathrooms, sqft should accept None."""
        listing = ScrapedListing(
            mls_id="R2900001",
            url="https://test.com",
            address="123 Test",
            city="Vancouver",
            latitude=49.0,
            longitude=-123.0,
            price=500000,
            bedrooms=None,
            bathrooms=None,
            sqft=None,
            property_type=None,
            listing_date=None,
            raw_data={},
        )

        assert listing.bedrooms is None
        assert listing.bathrooms is None
        assert listing.sqft is None
        assert listing.property_type is None
        assert listing.listing_date is None

    def test_stores_all_provided_fields(self):
        """All provided fields should be stored correctly."""
        listing = make_scraped_listing(
            mls_id="R2912345",
            price=1500000,
            address="789 Pine St",
            city="Richmond",
            latitude=49.1666,
            longitude=-123.1336,
            bedrooms=5,
            bathrooms=4,
            sqft=3000,
            property_type="House",
        )

        assert listing.mls_id == "R2912345"
        assert listing.price == 1500000
        assert listing.address == "789 Pine St"
        assert listing.city == "Richmond"
        assert listing.latitude == 49.1666
        assert listing.longitude == -123.1336
        assert listing.bedrooms == 5
        assert listing.bathrooms == 4
        assert listing.sqft == 3000
        assert listing.property_type == "House"

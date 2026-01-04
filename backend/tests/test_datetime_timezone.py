"""
Tests for timezone-aware datetime handling in the database.

The application should use timezone-aware datetimes (UTC) throughout.
This ensures consistency and avoids the deprecated datetime.utcnow().
"""

import pytest
from datetime import datetime, UTC

from sqlalchemy import select, text
from sqlalchemy.ext.asyncio import create_async_engine, async_sessionmaker

from app.config import settings
from app.models import Listing


@pytest.fixture
async def db_session():
    """Create a real database session for integration testing."""
    engine = create_async_engine(settings.database_url)
    async_session = async_sessionmaker(engine, expire_on_commit=False)
    async with async_session() as session:
        yield session
    await engine.dispose()


class TestListingDatetimeTimezone:
    """Tests for timezone handling in Listing model."""

    @pytest.mark.asyncio
    async def test_listing_accepts_timezone_aware_datetime(self, db_session):
        """Listing should accept timezone-aware datetime for last_seen."""
        # Create a listing with timezone-aware datetime
        now_utc = datetime.now(UTC)

        listing = Listing(
            mls_id=f"TEST-TZ-{now_utc.timestamp()}",
            url="https://test.com/tz-test",
            address="123 Timezone Test St",
            city="Vancouver",
            price=500000,
            status="active",
        )
        listing.first_seen = now_utc
        listing.last_seen = now_utc

        db_session.add(listing)
        await db_session.commit()

        # Refresh to get the value from database
        await db_session.refresh(listing)

        # Verify the datetime was stored and retrieved correctly
        assert listing.last_seen is not None
        # The stored datetime should be timezone-aware
        assert listing.last_seen.tzinfo is not None

        # Clean up
        await db_session.delete(listing)
        await db_session.commit()

    @pytest.mark.asyncio
    async def test_listing_last_seen_update_with_timezone_aware(self, db_session):
        """Updating last_seen with timezone-aware datetime should work."""
        now_utc = datetime.now(UTC)

        # Create listing
        listing = Listing(
            mls_id=f"TEST-TZ-UPDATE-{now_utc.timestamp()}",
            url="https://test.com/tz-update-test",
            address="456 Timezone Update St",
            city="Burnaby",
            price=600000,
            status="active",
        )
        db_session.add(listing)
        await db_session.commit()

        # Update last_seen with timezone-aware datetime (this is what the scraper does)
        listing.last_seen = datetime.now(UTC)
        await db_session.commit()

        # Verify it worked
        await db_session.refresh(listing)
        assert listing.last_seen is not None

        # Clean up
        await db_session.delete(listing)
        await db_session.commit()

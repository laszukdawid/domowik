"""Tests for single listing fetch functionality."""

import pytest
from unittest.mock import AsyncMock, MagicMock, patch

from scraper.realtor_ca import RealtorCaScraper


@pytest.mark.asyncio
async def test_fetch_single_success(realtor_ca_token_response, realtor_ca_listing_standard):
    """Test fetching a single listing by MLS ID."""
    scraper = RealtorCaScraper()

    # Mock the token and API responses
    with patch.object(scraper, '_fetch_reese84_token', new_callable=AsyncMock) as mock_token:
        mock_token.return_value = realtor_ca_token_response["token"]

        with patch.object(scraper.client, 'post', new_callable=AsyncMock) as mock_post:
            # Mock response for the API call
            mock_response = MagicMock()
            mock_response.raise_for_status = lambda: None
            mock_response.json.return_value = {
                "Results": [realtor_ca_listing_standard],
                "Paging": {"TotalRecords": 1}
            }
            mock_post.return_value = mock_response

            result = await scraper.fetch_single("R2912345")

            assert result is not None
            assert result.mls_id == "R2912345"
            assert result.price == 1299000
            assert result.address == "123 Main Street"

    await scraper.close()


@pytest.mark.asyncio
async def test_fetch_single_not_found(realtor_ca_token_response):
    """Test fetching a non-existent listing returns None."""
    scraper = RealtorCaScraper()

    with patch.object(scraper, '_fetch_reese84_token', new_callable=AsyncMock) as mock_token:
        mock_token.return_value = realtor_ca_token_response["token"]

        with patch.object(scraper.client, 'post', new_callable=AsyncMock) as mock_post:
            mock_response = MagicMock()
            mock_response.raise_for_status = lambda: None
            mock_response.json.return_value = {
                "Results": [],
                "Paging": {"TotalRecords": 0}
            }
            mock_post.return_value = mock_response

            result = await scraper.fetch_single("NONEXISTENT")

            assert result is None

    await scraper.close()

"""
Scraper for realtor.ca listings in Greater Vancouver Area.

Uses realtor.ca's API endpoint to fetch property listings.
"""

import asyncio
import random
from datetime import datetime
from typing import Any

import httpx
from pydantic import BaseModel


USER_AGENTS = [
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120.0.0.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/120.0.0.0",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/120.0.0.0",
]

# Greater Vancouver Area bounds (approximate)
GVA_BOUNDS = {
    "lat_min": 49.0,
    "lat_max": 49.4,
    "lng_min": -123.3,
    "lng_max": -122.5,
}


class ScrapedListing(BaseModel):
    mls_id: str
    url: str
    address: str
    city: str
    latitude: float
    longitude: float
    price: int
    bedrooms: int | None
    bathrooms: int | None
    sqft: int | None
    property_type: str | None
    listing_date: datetime | None
    raw_data: dict[str, Any]


class RealtorCaScraper:
    BASE_URL = "https://api2.realtor.ca/Listing.svc/PropertySearch_Post"

    def __init__(self):
        self.client = httpx.AsyncClient(timeout=30.0)

    async def close(self):
        await self.client.aclose()

    def _get_headers(self) -> dict[str, str]:
        return {
            "User-Agent": random.choice(USER_AGENTS),
            "Accept": "application/json",
            "Content-Type": "application/x-www-form-urlencoded",
            "Origin": "https://www.realtor.ca",
            "Referer": "https://www.realtor.ca/",
        }

    def _build_search_params(self, page: int = 1, records_per_page: int = 50) -> dict:
        return {
            "CultureId": "1",
            "ApplicationId": "1",
            "RecordsPerPage": str(records_per_page),
            "MaximumResults": str(records_per_page),
            "PropertyTypeGroupID": "1",  # Residential
            "TransactionTypeId": "2",  # For sale
            "LatitudeMin": str(GVA_BOUNDS["lat_min"]),
            "LatitudeMax": str(GVA_BOUNDS["lat_max"]),
            "LongitudeMin": str(GVA_BOUNDS["lng_min"]),
            "LongitudeMax": str(GVA_BOUNDS["lng_max"]),
            "CurrentPage": str(page),
            "SortBy": "1",  # Sort by date
            "SortOrder": "D",  # Descending
        }

    def _parse_listing(self, data: dict) -> ScrapedListing | None:
        try:
            mls_id = data.get("MlsNumber", "")
            if not mls_id:
                return None

            # Extract address
            address_parts = []
            if data.get("Property", {}).get("Address", {}).get("AddressText"):
                address_parts.append(
                    data["Property"]["Address"]["AddressText"].split("|")[0].strip()
                )

            address = ", ".join(address_parts) or "Unknown"
            city = (
                data.get("Property", {})
                .get("Address", {})
                .get("CityDistrict", "Unknown")
            )

            # Extract coordinates
            lat = float(data.get("Property", {}).get("Address", {}).get("Latitude", 0))
            lng = float(
                data.get("Property", {}).get("Address", {}).get("Longitude", 0)
            )

            # Extract price
            price_str = data.get("Property", {}).get("Price", "0")
            price = int("".join(filter(str.isdigit, str(price_str))) or 0)

            # Extract bedrooms/bathrooms
            bedrooms = None
            bathrooms = None
            building = data.get("Building", {})
            if building.get("Bedrooms"):
                bedrooms = int(building["Bedrooms"])
            if building.get("BathroomTotal"):
                bathrooms = int(building["BathroomTotal"])

            # Extract sqft
            sqft = None
            size_interior = building.get("SizeInterior")
            if size_interior:
                # Parse "1,234 sqft" format
                sqft_str = "".join(filter(str.isdigit, size_interior.split()[0]))
                if sqft_str:
                    sqft = int(sqft_str)

            # Property type
            property_type = building.get("Type")

            # Listing date
            listing_date = None
            posted_date = data.get("PostedDate")
            if posted_date:
                try:
                    listing_date = datetime.fromisoformat(
                        posted_date.replace("Z", "+00:00")
                    )
                except ValueError:
                    pass

            return ScrapedListing(
                mls_id=mls_id,
                url=f"https://www.realtor.ca/real-estate/{mls_id}",
                address=address,
                city=city,
                latitude=lat,
                longitude=lng,
                price=price,
                bedrooms=bedrooms,
                bathrooms=bathrooms,
                sqft=sqft,
                property_type=property_type,
                listing_date=listing_date,
                raw_data=data,
            )
        except Exception as e:
            print(f"Error parsing listing: {e}")
            return None

    async def fetch_page(self, page: int = 1) -> tuple[list[ScrapedListing], int]:
        """Fetch a page of listings. Returns (listings, total_count)."""
        params = self._build_search_params(page=page)

        try:
            response = await self.client.post(
                self.BASE_URL,
                data=params,
                headers=self._get_headers(),
            )
            response.raise_for_status()
            data = response.json()

            results = data.get("Results", [])
            total = int(data.get("Paging", {}).get("TotalRecords", 0))

            listings = []
            for item in results:
                listing = self._parse_listing(item)
                if listing:
                    listings.append(listing)

            return listings, total

        except httpx.HTTPError as e:
            print(f"HTTP error fetching page {page}: {e}")
            return [], 0

    async def fetch_all(self, max_pages: int = 100) -> list[ScrapedListing]:
        """Fetch all listings, respecting rate limits."""
        all_listings = []

        listings, total = await self.fetch_page(1)
        all_listings.extend(listings)
        print(f"Page 1: {len(listings)} listings (total: {total})")

        if total == 0:
            return all_listings

        total_pages = min((total // 50) + 1, max_pages)

        for page in range(2, total_pages + 1):
            # Rate limiting: 2-3 second delay
            await asyncio.sleep(random.uniform(2.0, 3.0))

            listings, _ = await self.fetch_page(page)
            all_listings.extend(listings)
            print(f"Page {page}/{total_pages}: {len(listings)} listings")

        return all_listings

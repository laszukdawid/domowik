"""
Scraper for realtor.ca listings in Greater Vancouver Area.

Uses realtor.ca's API endpoint to fetch property listings.
Based on pyRealtor's approach for handling authentication.
"""

import asyncio
import random
from datetime import datetime, timezone
from typing import Any

import httpx
from pydantic import BaseModel


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
    TOKEN_URL = "https://www.realtor.ca/dnight-Exit-shall-Braith-Then-why-vponst-is-proc"

    def __init__(self):
        self.client = httpx.AsyncClient(timeout=30.0)
        self._reese84_token: str | None = None

    async def close(self):
        await self.client.aclose()

    async def _fetch_reese84_token(self) -> str:
        """Fetch the reese84 authentication token required by realtor.ca."""
        payload = {
            "solution": {
                "interrogation": None,
                "version": "beta"
            },
            "old_token": None,
            "error": None,
            "performance": {"interrogation": 1897}
        }

        response = await self.client.post(
            self.TOKEN_URL,
            json=payload,
            params={"d": "www.realtor.ca"}
        )
        response.raise_for_status()
        data = response.json()
        return data["token"]

    def _get_headers(self, token: str) -> dict[str, str]:
        return {
            "Accept": "*/*",
            "Accept-Encoding": "gzip, deflate, br",
            "Accept-Language": "en-US,en;q=0.9",
            "Cache-Control": "no-cache",
            "Connection": "keep-alive",
            "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8",
            "DNT": "1",
            "Host": "api2.realtor.ca",
            "Origin": "https://www.realtor.ca",
            "Pragma": "no-cache",
            "Referer": "https://www.realtor.ca/",
            "Sec-Fetch-Dest": "empty",
            "Sec-Fetch-Mode": "cors",
            "Sec-Fetch-Site": "same-site",
            "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36",
            "Cookie": f"reese84={token};",
        }

    def _build_search_params(self, page: int = 1, records_per_page: int = 200) -> dict:
        return {
            "Version": "7.0",
            "ApplicationId": "1",
            "CultureId": "1",
            "Currency": "CAD",
            "RecordsPerPage": str(records_per_page),
            "MaximumResults": "600",
            "PropertyTypeGroupID": "1",  # Residential
            "TransactionTypeId": "2",  # For sale
            "LatitudeMin": str(GVA_BOUNDS["lat_min"]),
            "LatitudeMax": str(GVA_BOUNDS["lat_max"]),
            "LongitudeMin": str(GVA_BOUNDS["lng_min"]),
            "LongitudeMax": str(GVA_BOUNDS["lng_max"]),
            "CurrentPage": str(page),
            "ZoomLevel": "10",
            "Sort": "6-D",  # Sort by date descending
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

            # Extract city from AddressText (format: "Address|City, Province PostalCode")
            address_text = data.get("Property", {}).get("Address", {}).get("AddressText", "")
            city = "Unknown"
            if "|" in address_text:
                city_part = address_text.split("|")[1].strip()
                # City is before the comma (e.g., "Vancouver, British Columbia V6B1A1")
                if "," in city_part:
                    city = city_part.split(",")[0].strip()
                else:
                    city = city_part

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
                # Handle formats like "2 + 1" by taking first number
                bed_str = str(building["Bedrooms"]).split("+")[0].strip()
                if bed_str.isdigit():
                    bedrooms = int(bed_str)
            if building.get("BathroomTotal"):
                bath_str = str(building["BathroomTotal"]).split("+")[0].strip()
                if bath_str.isdigit():
                    bathrooms = int(bath_str)

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

            # Build URL from RelativeURLEn if available, otherwise use listing Id
            relative_url = data.get("RelativeURLEn", "")
            if relative_url:
                url = f"https://www.realtor.ca{relative_url}"
            else:
                listing_id = data.get("Id", mls_id)
                url = f"https://www.realtor.ca/real-estate/{listing_id}"

            return ScrapedListing(
                mls_id=mls_id,
                url=url,
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
        # Get fresh token for each request
        token = await self._fetch_reese84_token()
        params = self._build_search_params(page=page)

        try:
            response = await self.client.post(
                self.BASE_URL,
                data=params,
                headers=self._get_headers(token),
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

        total_pages = min((total // 200) + 1, max_pages)

        for page in range(2, total_pages + 1):
            # Rate limiting: 2-3 second delay
            await asyncio.sleep(random.uniform(2.0, 3.0))

            listings, _ = await self.fetch_page(page)
            all_listings.extend(listings)
            print(f"Page {page}/{total_pages}: {len(listings)} listings")

        return all_listings

    async def fetch_single(self, mls_id: str) -> ScrapedListing | None:
        """Fetch a single listing by MLS ID.

        Args:
            mls_id: The MLS number to search for

        Returns:
            ScrapedListing if found, None otherwise
        """
        token = await self._fetch_reese84_token()

        # Use MLS number search params
        params = {
            "Version": "7.0",
            "ApplicationId": "1",
            "CultureId": "1",
            "Currency": "CAD",
            "RecordsPerPage": "1",
            "MaximumResults": "1",
            "PropertyTypeGroupID": "1",
            "TransactionTypeId": "2",
            "ReferenceNumber": mls_id,  # Search by MLS number
        }

        try:
            response = await self.client.post(
                self.BASE_URL,
                data=params,
                headers=self._get_headers(token),
            )
            response.raise_for_status()
            data = response.json()

            results = data.get("Results", [])
            if not results:
                return None

            return self._parse_listing(results[0])

        except httpx.HTTPError as e:
            print(f"HTTP error fetching MLS {mls_id}: {e}")
            return None

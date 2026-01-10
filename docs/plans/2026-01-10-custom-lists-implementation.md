# Custom Lists Feature Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enable users to create multiple named lists of manually-added listings, with URL/MLS input, that filter the map view.

**Architecture:** New `custom_lists` and `custom_list_listings` tables. New REST API for CRUD operations. Frontend adds list selector dropdown in sidebar, integrates with existing listing/cluster hooks via `custom_list_id` filter param. On-demand scraping via existing `RealtorCaScraper`.

**Tech Stack:** FastAPI, SQLAlchemy async, PostgreSQL, Alembic migrations, React, TypeScript, TanStack Query

---

## Task 1: Database Migration

**Files:**
- Create: `backend/alembic/versions/002_add_custom_lists.py`

**Step 1: Create migration file**

```python
"""Add custom lists tables

Revision ID: 002
Revises: e31c29c62f1f
Create Date: 2026-01-10
"""
from typing import Sequence, Union
from alembic import op
import sqlalchemy as sa

revision: str = "002"
down_revision: Union[str, None] = "e31c29c62f1f"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.create_table(
        "custom_lists",
        sa.Column("id", sa.Integer(), nullable=False),
        sa.Column("name", sa.Text(), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.PrimaryKeyConstraint("id"),
    )

    op.create_table(
        "custom_list_listings",
        sa.Column("custom_list_id", sa.Integer(), nullable=False),
        sa.Column("listing_id", sa.Integer(), nullable=False),
        sa.Column("source_url", sa.Text(), nullable=True),
        sa.Column("added_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.ForeignKeyConstraint(["custom_list_id"], ["custom_lists.id"], ondelete="CASCADE"),
        sa.ForeignKeyConstraint(["listing_id"], ["listings.id"], ondelete="CASCADE"),
        sa.PrimaryKeyConstraint("custom_list_id", "listing_id"),
    )
    op.create_index("ix_custom_list_listings_list", "custom_list_listings", ["custom_list_id"])
    op.create_index("ix_custom_list_listings_listing", "custom_list_listings", ["listing_id"])


def downgrade() -> None:
    op.drop_table("custom_list_listings")
    op.drop_table("custom_lists")
```

**Step 2: Run migration**

Run: `cd backend && alembic upgrade head`
Expected: Migration applies successfully, tables created

**Step 3: Commit**

```bash
git add backend/alembic/versions/002_add_custom_lists.py
git commit -m "feat: add custom_lists database migration"
```

---

## Task 2: SQLAlchemy Models

**Files:**
- Create: `backend/app/models/custom_list.py`
- Modify: `backend/app/models/__init__.py`

**Step 1: Create model file**

```python
from datetime import datetime, UTC
from sqlalchemy import String, Integer, DateTime, Text, ForeignKey
from sqlalchemy.orm import Mapped, mapped_column, relationship

from app.models.base import Base


def utc_now() -> datetime:
    return datetime.now(UTC)


class CustomList(Base):
    __tablename__ = "custom_lists"

    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str] = mapped_column(Text, nullable=False)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utc_now)

    listings: Mapped[list["CustomListListing"]] = relationship(
        back_populates="custom_list", cascade="all, delete-orphan"
    )


class CustomListListing(Base):
    __tablename__ = "custom_list_listings"

    custom_list_id: Mapped[int] = mapped_column(
        ForeignKey("custom_lists.id", ondelete="CASCADE"), primary_key=True
    )
    listing_id: Mapped[int] = mapped_column(
        ForeignKey("listings.id", ondelete="CASCADE"), primary_key=True
    )
    source_url: Mapped[str | None] = mapped_column(Text, nullable=True)
    added_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utc_now)

    custom_list: Mapped["CustomList"] = relationship(back_populates="listings")
    listing: Mapped["Listing"] = relationship()


from app.models.listing import Listing  # noqa: E402, F401
```

**Step 2: Update models __init__.py**

Add to imports and __all__ in `backend/app/models/__init__.py`:

```python
from app.models.custom_list import CustomList, CustomListListing
```

Add to __all__:
```python
"CustomList",
"CustomListListing",
```

**Step 3: Run tests to verify models load**

Run: `cd backend && python -c "from app.models import CustomList, CustomListListing; print('OK')"`
Expected: `OK`

**Step 4: Commit**

```bash
git add backend/app/models/custom_list.py backend/app/models/__init__.py
git commit -m "feat: add CustomList and CustomListListing models"
```

---

## Task 3: Pydantic Schemas for Custom Lists

**Files:**
- Create: `backend/app/schemas/custom_list.py`

**Step 1: Create schema file**

```python
from datetime import datetime
from pydantic import BaseModel


class CustomListCreate(BaseModel):
    name: str | None = None


class CustomListUpdate(BaseModel):
    name: str


class CustomListResponse(BaseModel):
    id: int
    name: str
    count: int
    created_at: datetime

    class Config:
        from_attributes = True


class AddListingRequest(BaseModel):
    input: str  # URL or MLS number


class AddListingResponse(BaseModel):
    listing_id: int
    status: str  # "created" or "existing"
```

**Step 2: Commit**

```bash
git add backend/app/schemas/custom_list.py
git commit -m "feat: add custom list Pydantic schemas"
```

---

## Task 4: Scraper - Add fetch_single Method

**Files:**
- Modify: `backend/scraper/realtor_ca.py`
- Create: `backend/tests/scraper/test_fetch_single.py`

**Step 1: Write failing test**

Create `backend/tests/scraper/test_fetch_single.py`:

```python
"""Tests for single listing fetch functionality."""

import pytest
from unittest.mock import AsyncMock, patch

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
            mock_response = AsyncMock()
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
            mock_response = AsyncMock()
            mock_response.raise_for_status = lambda: None
            mock_response.json.return_value = {
                "Results": [],
                "Paging": {"TotalRecords": 0}
            }
            mock_post.return_value = mock_response

            result = await scraper.fetch_single("NONEXISTENT")

            assert result is None

    await scraper.close()
```

**Step 2: Run test to verify it fails**

Run: `cd backend && pytest tests/scraper/test_fetch_single.py -v`
Expected: FAIL with `AttributeError: 'RealtorCaScraper' object has no attribute 'fetch_single'`

**Step 3: Implement fetch_single method**

Add to `backend/scraper/realtor_ca.py` after the `fetch_all` method:

```python
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
```

**Step 4: Run test to verify it passes**

Run: `cd backend && pytest tests/scraper/test_fetch_single.py -v`
Expected: PASS

**Step 5: Commit**

```bash
git add backend/scraper/realtor_ca.py backend/tests/scraper/test_fetch_single.py
git commit -m "feat: add fetch_single method to RealtorCaScraper"
```

---

## Task 5: Custom Lists API - CRUD Endpoints

**Files:**
- Create: `backend/app/api/custom_lists.py`
- Modify: `backend/app/api/__init__.py`
- Modify: `backend/app/main.py`

**Step 1: Create the API router**

Create `backend/app/api/custom_lists.py`:

```python
import re
import logging
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, BackgroundTasks
from sqlalchemy import select, func
from sqlalchemy.ext.asyncio import AsyncSession

from app.models import get_db, Listing, CustomList, CustomListListing
from app.schemas.custom_list import (
    CustomListCreate,
    CustomListUpdate,
    CustomListResponse,
    AddListingRequest,
    AddListingResponse,
)
from scraper.realtor_ca import RealtorCaScraper
from scraper.enricher import AmenityEnricher

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/custom-lists", tags=["custom-lists"])


async def get_next_list_number(db: AsyncSession) -> int:
    """Get the next available list number for auto-naming."""
    result = await db.execute(
        select(func.count()).select_from(CustomList)
    )
    return result.scalar() + 1


def parse_mls_from_url(url: str) -> str | None:
    """Extract MLS ID from a realtor.ca URL.

    Handles formats like:
    - https://www.realtor.ca/real-estate/12345678/123-main-st-vancouver
    - https://realtor.ca/real-estate/12345678/...
    """
    # Match the numeric ID after /real-estate/
    match = re.search(r'realtor\.ca/real-estate/(\d+)', url)
    if match:
        return match.group(1)
    return None


def validate_mls_number(value: str) -> bool:
    """Check if value looks like a valid MLS number."""
    # MLS numbers are typically alphanumeric, 6-10 characters
    return bool(re.match(r'^[A-Za-z0-9]{6,15}$', value.strip()))


@router.get("", response_model=list[CustomListResponse])
async def get_custom_lists(
    db: Annotated[AsyncSession, Depends(get_db)],
):
    """Get all custom lists with their listing counts."""
    query = (
        select(
            CustomList.id,
            CustomList.name,
            CustomList.created_at,
            func.count(CustomListListing.listing_id).label("count"),
        )
        .outerjoin(CustomListListing, CustomList.id == CustomListListing.custom_list_id)
        .group_by(CustomList.id)
        .order_by(CustomList.created_at.desc())
    )

    result = await db.execute(query)
    rows = result.all()

    return [
        CustomListResponse(
            id=row.id,
            name=row.name,
            count=row.count,
            created_at=row.created_at,
        )
        for row in rows
    ]


@router.post("", response_model=CustomListResponse)
async def create_custom_list(
    db: Annotated[AsyncSession, Depends(get_db)],
    body: CustomListCreate | None = None,
):
    """Create a new custom list."""
    name = body.name if body and body.name else None

    if not name:
        # Auto-generate name
        num = await get_next_list_number(db)
        name = f"Custom Listing #{num}"

    custom_list = CustomList(name=name)
    db.add(custom_list)
    await db.commit()
    await db.refresh(custom_list)

    return CustomListResponse(
        id=custom_list.id,
        name=custom_list.name,
        count=0,
        created_at=custom_list.created_at,
    )


@router.patch("/{list_id}", response_model=CustomListResponse)
async def update_custom_list(
    list_id: int,
    body: CustomListUpdate,
    db: Annotated[AsyncSession, Depends(get_db)],
):
    """Rename a custom list."""
    result = await db.execute(
        select(CustomList).where(CustomList.id == list_id)
    )
    custom_list = result.scalar_one_or_none()

    if not custom_list:
        raise HTTPException(status_code=404, detail="List not found")

    custom_list.name = body.name
    await db.commit()
    await db.refresh(custom_list)

    # Get count
    count_result = await db.execute(
        select(func.count()).select_from(CustomListListing).where(
            CustomListListing.custom_list_id == list_id
        )
    )
    count = count_result.scalar()

    return CustomListResponse(
        id=custom_list.id,
        name=custom_list.name,
        count=count,
        created_at=custom_list.created_at,
    )


@router.delete("/{list_id}")
async def delete_custom_list(
    list_id: int,
    db: Annotated[AsyncSession, Depends(get_db)],
):
    """Delete a custom list."""
    result = await db.execute(
        select(CustomList).where(CustomList.id == list_id)
    )
    custom_list = result.scalar_one_or_none()

    if not custom_list:
        raise HTTPException(status_code=404, detail="List not found")

    await db.delete(custom_list)
    await db.commit()

    return {"status": "deleted"}


async def enrich_listing_background(listing_id: int):
    """Background task to enrich a listing with amenity data."""
    from app.models.base import async_session

    try:
        enricher = AmenityEnricher()
        async with async_session() as db:
            result = await db.execute(
                select(Listing).where(Listing.id == listing_id)
            )
            listing = result.scalar_one_or_none()
            if listing and listing.location:
                from geoalchemy2.functions import ST_X, ST_Y
                coord_result = await db.execute(
                    select(ST_X(Listing.location), ST_Y(Listing.location)).where(
                        Listing.id == listing_id
                    )
                )
                coords = coord_result.first()
                if coords:
                    lng, lat = coords
                    await enricher.enrich_listing(db, listing, lat, lng)
                    await db.commit()
        await enricher.close()
    except Exception as e:
        logger.error(f"Background enrichment failed for listing {listing_id}: {e}")


@router.post("/{list_id}/listings", response_model=AddListingResponse)
async def add_listing_to_list(
    list_id: int,
    body: AddListingRequest,
    background_tasks: BackgroundTasks,
    db: Annotated[AsyncSession, Depends(get_db)],
):
    """Add a listing to a custom list by URL or MLS number."""
    # Verify list exists
    list_result = await db.execute(
        select(CustomList).where(CustomList.id == list_id)
    )
    if not list_result.scalar_one_or_none():
        raise HTTPException(status_code=404, detail="List not found")

    input_value = body.input.strip()
    mls_id: str | None = None
    source_url: str | None = None

    # Determine if input is URL or MLS number
    if input_value.startswith("http://") or input_value.startswith("https://"):
        # Validate it's a realtor.ca URL
        if "realtor.ca" not in input_value.lower():
            raise HTTPException(
                status_code=400,
                detail="Only realtor.ca listing URLs are supported"
            )

        mls_id = parse_mls_from_url(input_value)
        if not mls_id:
            raise HTTPException(
                status_code=400,
                detail="Couldn't find listing ID in URL"
            )
        source_url = input_value
    else:
        # Treat as MLS number
        if not validate_mls_number(input_value):
            raise HTTPException(
                status_code=400,
                detail="Please enter a valid MLS number"
            )
        mls_id = input_value.upper()

    # Check if listing already exists in database
    result = await db.execute(
        select(Listing).where(Listing.mls_id == mls_id)
    )
    listing = result.scalar_one_or_none()
    status = "existing"

    if listing:
        # Listing exists, just add to list
        pass
    elif source_url:
        # Need to fetch from realtor.ca API
        scraper = RealtorCaScraper()
        try:
            scraped = await scraper.fetch_single(mls_id)
            if not scraped:
                raise HTTPException(
                    status_code=404,
                    detail="Listing not found - it may have been removed"
                )

            # Insert new listing
            from geoalchemy2.functions import ST_SetSRID, ST_MakePoint

            listing = Listing(
                mls_id=scraped.mls_id,
                url=scraped.url,
                address=scraped.address,
                city=scraped.city,
                location=ST_SetSRID(ST_MakePoint(scraped.longitude, scraped.latitude), 4326),
                price=scraped.price,
                bedrooms=scraped.bedrooms,
                bathrooms=scraped.bathrooms,
                sqft=scraped.sqft,
                property_type=scraped.property_type,
                listing_date=scraped.listing_date.date() if scraped.listing_date else None,
                raw_data=scraped.raw_data,
            )
            db.add(listing)
            await db.commit()
            await db.refresh(listing)
            status = "created"

            # Trigger background enrichment
            background_tasks.add_task(enrich_listing_background, listing.id)

        finally:
            await scraper.close()
    else:
        # MLS number only, but not in database
        raise HTTPException(
            status_code=404,
            detail="Listing not found. Try pasting the full realtor.ca URL instead."
        )

    # Check if already in this list
    existing_link = await db.execute(
        select(CustomListListing).where(
            CustomListListing.custom_list_id == list_id,
            CustomListListing.listing_id == listing.id,
        )
    )
    if not existing_link.scalar_one_or_none():
        # Add to list
        link = CustomListListing(
            custom_list_id=list_id,
            listing_id=listing.id,
            source_url=source_url,
        )
        db.add(link)
        await db.commit()

    return AddListingResponse(listing_id=listing.id, status=status)


@router.delete("/{list_id}/listings/{listing_id}")
async def remove_listing_from_list(
    list_id: int,
    listing_id: int,
    db: Annotated[AsyncSession, Depends(get_db)],
):
    """Remove a listing from a custom list."""
    result = await db.execute(
        select(CustomListListing).where(
            CustomListListing.custom_list_id == list_id,
            CustomListListing.listing_id == listing_id,
        )
    )
    link = result.scalar_one_or_none()

    if not link:
        raise HTTPException(status_code=404, detail="Listing not in this list")

    await db.delete(link)
    await db.commit()

    return {"status": "removed"}
```

**Step 2: Update API __init__.py**

Add to `backend/app/api/__init__.py`:

```python
from app.api.custom_lists import router as custom_lists_router
```

Add to __all__:
```python
"custom_lists_router",
```

**Step 3: Update main.py**

Add to `backend/app/main.py` imports:
```python
custom_lists_router,
```

Add router:
```python
app.include_router(custom_lists_router, prefix="/api")
```

**Step 4: Test the endpoints manually**

Run: `cd backend && uvicorn app.main:app --reload`

Test with curl:
```bash
# Create a list
curl -X POST http://localhost:8000/api/custom-lists -H "Content-Type: application/json"

# Get all lists
curl http://localhost:8000/api/custom-lists
```

Expected: 200 responses with correct data

**Step 5: Commit**

```bash
git add backend/app/api/custom_lists.py backend/app/api/__init__.py backend/app/main.py
git commit -m "feat: add custom lists API endpoints"
```

---

## Task 6: Modify Listings/Clusters Endpoints for custom_list_id Filter

**Files:**
- Modify: `backend/app/api/listings.py`
- Modify: `backend/app/api/clusters.py`

**Step 1: Update build_listings_query_with_groups in listings.py**

Add `custom_list_id` parameter to the function and add JOIN + WHERE clause:

```python
def build_listings_query_with_groups(
    user: User,
    filter_groups: list[FilterGroup],
    include_hidden: bool = False,
    favorites_only: bool = False,
    bbox: str | None = None,
    polygons: list[list[list[float]]] | None = None,
    custom_list_id: int | None = None,  # NEW PARAMETER
):
```

Add after the initial query definition (after `.where(Listing.status == "active")`):

```python
    # Filter by custom list if specified
    if custom_list_id is not None:
        from app.models import CustomListListing
        query = query.join(
            CustomListListing,
            CustomListListing.listing_id == Listing.id
        ).where(CustomListListing.custom_list_id == custom_list_id)
```

**Step 2: Update FilterGroups schema**

Add to `backend/app/schemas/listing.py` FilterGroups class:

```python
    custom_list_id: int | None = None
```

**Step 3: Update stream_listings_with_groups endpoint**

Pass `custom_list_id` to the query builder:

```python
query = build_listings_query_with_groups(
    user, filter_groups.groups, filter_groups.include_hidden,
    filter_groups.favorites_only, bbox, filter_groups.polygons,
    filter_groups.custom_list_id  # NEW
)
```

**Step 4: Update get_listings_with_groups endpoint**

Same change - pass `filter_groups.custom_list_id` to `build_listings_query_with_groups`.

**Step 5: Update clusters.py similarly**

Add `custom_list_id` parameter to `build_lightweight_query_with_groups` function with the same JOIN logic.

Update the FilterGroups body in `get_clusters_with_groups` to pass `filter_groups.custom_list_id`.

**Step 6: Test the filter works**

Run manual test with curl to verify custom_list_id filtering works.

**Step 7: Commit**

```bash
git add backend/app/api/listings.py backend/app/api/clusters.py backend/app/schemas/listing.py
git commit -m "feat: add custom_list_id filter to listings and clusters endpoints"
```

---

## Task 7: Frontend Types

**Files:**
- Modify: `frontend/src/types/index.ts`

**Step 1: Add custom list types**

Add to `frontend/src/types/index.ts`:

```typescript
export interface CustomList {
  id: number;
  name: string;
  count: number;
  created_at: string;
}

export interface AddListingResult {
  listing_id: number;
  status: 'created' | 'existing';
}
```

**Step 2: Update FilterGroups interface**

Add to the existing `FilterGroups` interface:

```typescript
  custom_list_id?: number;
```

**Step 3: Commit**

```bash
git add frontend/src/types/index.ts
git commit -m "feat: add custom list TypeScript types"
```

---

## Task 8: Frontend API Client

**Files:**
- Modify: `frontend/src/api/client.ts`

**Step 1: Add custom list API methods**

Add to the `ApiClient` class in `frontend/src/api/client.ts`:

```typescript
  // Custom Lists
  async getCustomLists(): Promise<CustomList[]> {
    return this.request<CustomList[]>('/api/custom-lists');
  }

  async createCustomList(name?: string): Promise<CustomList> {
    return this.request<CustomList>('/api/custom-lists', {
      method: 'POST',
      body: JSON.stringify(name ? { name } : {}),
    });
  }

  async updateCustomList(id: number, name: string): Promise<CustomList> {
    return this.request<CustomList>(`/api/custom-lists/${id}`, {
      method: 'PATCH',
      body: JSON.stringify({ name }),
    });
  }

  async deleteCustomList(id: number): Promise<void> {
    await this.request(`/api/custom-lists/${id}`, {
      method: 'DELETE',
    });
  }

  async addListingToCustomList(listId: number, input: string): Promise<AddListingResult> {
    return this.request<AddListingResult>(`/api/custom-lists/${listId}/listings`, {
      method: 'POST',
      body: JSON.stringify({ input }),
    });
  }

  async removeListingFromCustomList(listId: number, listingId: number): Promise<void> {
    await this.request(`/api/custom-lists/${listId}/listings/${listingId}`, {
      method: 'DELETE',
    });
  }
```

**Step 2: Add import for CustomList type**

Update the import at the top:

```typescript
import type { Listing, Note, Preferences, ListingFilters, ClusterResponse, POI, FilterGroups, CustomList, AddListingResult } from '../types';
```

**Step 3: Commit**

```bash
git add frontend/src/api/client.ts
git commit -m "feat: add custom list API client methods"
```

---

## Task 9: Frontend Hooks for Custom Lists

**Files:**
- Create: `frontend/src/hooks/useCustomLists.ts`

**Step 1: Create the hooks file**

```typescript
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api } from '../api/client';

export function useCustomLists() {
  return useQuery({
    queryKey: ['customLists'],
    queryFn: () => api.getCustomLists(),
  });
}

export function useCreateCustomList() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (name?: string) => api.createCustomList(name),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['customLists'] });
    },
  });
}

export function useUpdateCustomList() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ id, name }: { id: number; name: string }) =>
      api.updateCustomList(id, name),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['customLists'] });
    },
  });
}

export function useDeleteCustomList() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: number) => api.deleteCustomList(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['customLists'] });
    },
  });
}

export function useAddListingToCustomList() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ listId, input }: { listId: number; input: string }) =>
      api.addListingToCustomList(listId, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['customLists'] });
      queryClient.invalidateQueries({ queryKey: ['clusters'] });
    },
  });
}

export function useRemoveListingFromCustomList() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ listId, listingId }: { listId: number; listingId: number }) =>
      api.removeListingFromCustomList(listId, listingId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['customLists'] });
      queryClient.invalidateQueries({ queryKey: ['clusters'] });
    },
  });
}
```

**Step 2: Commit**

```bash
git add frontend/src/hooks/useCustomLists.ts
git commit -m "feat: add custom list React hooks"
```

---

## Task 10: ListSelector Component

**Files:**
- Create: `frontend/src/components/ListSelector.tsx`

**Step 1: Create the component**

```typescript
import { useState, useRef, useEffect } from 'react';
import type { CustomList } from '../types';

interface ListSelectorProps {
  customLists: CustomList[];
  selectedListId: number | null;
  onSelect: (listId: number | null) => void;
  onCreateList: () => void;
  onRenameList: (id: number, name: string) => void;
  onDeleteList: (id: number) => void;
}

export default function ListSelector({
  customLists,
  selectedListId,
  onSelect,
  onCreateList,
  onRenameList,
  onDeleteList,
}: ListSelectorProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [filter, setFilter] = useState('');
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editingName, setEditingName] = useState('');
  const dropdownRef = useRef<HTMLDivElement>(null);

  // Close dropdown when clicking outside
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsOpen(false);
        setEditingId(null);
      }
    }
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const selectedName = selectedListId === null
    ? 'All Listings'
    : customLists.find(l => l.id === selectedListId)?.name ?? 'All Listings';

  const filteredLists = customLists.filter(list =>
    list.name.toLowerCase().includes(filter.toLowerCase())
  );

  const handleStartEdit = (e: React.MouseEvent, list: CustomList) => {
    e.stopPropagation();
    setEditingId(list.id);
    setEditingName(list.name);
  };

  const handleSaveEdit = (e: React.FormEvent) => {
    e.preventDefault();
    if (editingId && editingName.trim()) {
      onRenameList(editingId, editingName.trim());
      setEditingId(null);
    }
  };

  const handleDelete = (e: React.MouseEvent, id: number) => {
    e.stopPropagation();
    if (confirm('Delete this list? Listings will not be deleted.')) {
      onDeleteList(id);
      if (selectedListId === id) {
        onSelect(null);
      }
    }
  };

  return (
    <div className="relative" ref={dropdownRef}>
      <div className="flex items-center gap-2">
        <button
          onClick={() => setIsOpen(!isOpen)}
          className="flex items-center gap-2 px-3 py-1.5 bg-white border rounded-md hover:bg-gray-50 text-sm font-medium"
        >
          <span className="truncate max-w-[150px]">{selectedName}</span>
          <svg className="w-4 h-4 text-gray-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
          </svg>
        </button>
        <button
          onClick={onCreateList}
          className="px-2 py-1.5 text-sm bg-blue-500 text-white rounded-md hover:bg-blue-600"
          title="Create new list"
        >
          + New
        </button>
      </div>

      {isOpen && (
        <div className="absolute top-full left-0 mt-1 w-72 bg-white border rounded-lg shadow-lg z-50">
          {/* Filter input */}
          <div className="p-2 border-b">
            <input
              type="text"
              placeholder="Filter lists..."
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              className="w-full px-2 py-1 text-sm border rounded focus:outline-none focus:ring-1 focus:ring-blue-500"
              autoFocus
            />
          </div>

          {/* List options */}
          <div className="max-h-64 overflow-y-auto">
            {/* All Listings option */}
            <button
              onClick={() => {
                onSelect(null);
                setIsOpen(false);
              }}
              className={`w-full px-3 py-2 text-left text-sm hover:bg-gray-50 flex items-center justify-between ${
                selectedListId === null ? 'bg-blue-50 text-blue-700' : ''
              }`}
            >
              <span>All Listings</span>
              {selectedListId === null && (
                <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                  <path fillRule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clipRule="evenodd" />
                </svg>
              )}
            </button>

            {/* Custom lists */}
            {filteredLists.map((list) => (
              <div
                key={list.id}
                className={`px-3 py-2 hover:bg-gray-50 ${
                  selectedListId === list.id ? 'bg-blue-50' : ''
                }`}
              >
                {editingId === list.id ? (
                  <form onSubmit={handleSaveEdit} className="flex items-center gap-2">
                    <input
                      type="text"
                      value={editingName}
                      onChange={(e) => setEditingName(e.target.value)}
                      className="flex-1 px-2 py-0.5 text-sm border rounded focus:outline-none focus:ring-1 focus:ring-blue-500"
                      autoFocus
                      onKeyDown={(e) => {
                        if (e.key === 'Escape') {
                          setEditingId(null);
                        }
                      }}
                    />
                    <button type="submit" className="text-green-600 hover:text-green-700">
                      <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                      </svg>
                    </button>
                  </form>
                ) : (
                  <button
                    onClick={() => {
                      onSelect(list.id);
                      setIsOpen(false);
                    }}
                    className="w-full flex items-center justify-between text-sm"
                  >
                    <span className={`truncate ${selectedListId === list.id ? 'text-blue-700 font-medium' : ''}`}>
                      {list.name}
                      <span className="text-gray-400 ml-1">({list.count})</span>
                    </span>
                    <div className="flex items-center gap-1">
                      {selectedListId === list.id && (
                        <svg className="w-4 h-4 text-blue-700" fill="currentColor" viewBox="0 0 20 20">
                          <path fillRule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clipRule="evenodd" />
                        </svg>
                      )}
                      <button
                        onClick={(e) => handleStartEdit(e, list)}
                        className="p-1 text-gray-400 hover:text-gray-600"
                        title="Rename"
                      >
                        <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
                        </svg>
                      </button>
                      <button
                        onClick={(e) => handleDelete(e, list.id)}
                        className="p-1 text-gray-400 hover:text-red-600"
                        title="Delete"
                      >
                        <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                        </svg>
                      </button>
                    </div>
                  </button>
                )}
              </div>
            ))}

            {filteredLists.length === 0 && filter && (
              <div className="px-3 py-2 text-sm text-gray-500">
                No lists match "{filter}"
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
```

**Step 2: Commit**

```bash
git add frontend/src/components/ListSelector.tsx
git commit -m "feat: add ListSelector dropdown component"
```

---

## Task 11: CustomListingInput Component

**Files:**
- Create: `frontend/src/components/CustomListingInput.tsx`

**Step 1: Create the component**

```typescript
import { useState } from 'react';

interface CustomListingInputProps {
  onSubmit: (input: string) => Promise<void>;
  isLoading?: boolean;
  error?: string | null;
}

export default function CustomListingInput({
  onSubmit,
  isLoading = false,
  error,
}: CustomListingInputProps) {
  const [input, setInput] = useState('');

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || isLoading) return;

    await onSubmit(input.trim());
    setInput('');
  };

  return (
    <form onSubmit={handleSubmit} className="flex gap-2">
      <input
        type="text"
        value={input}
        onChange={(e) => setInput(e.target.value)}
        placeholder="Paste realtor.ca URL or MLS number"
        className="flex-1 px-3 py-2 text-sm border rounded-md focus:outline-none focus:ring-1 focus:ring-blue-500"
        disabled={isLoading}
      />
      <button
        type="submit"
        disabled={!input.trim() || isLoading}
        className="px-3 py-2 text-sm bg-blue-500 text-white rounded-md hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {isLoading ? 'Adding...' : 'Add'}
      </button>
    </form>
  );
}
```

**Step 2: Commit**

```bash
git add frontend/src/components/CustomListingInput.tsx
git commit -m "feat: add CustomListingInput component"
```

---

## Task 12: Update ListingSidebar for Custom Lists

**Files:**
- Modify: `frontend/src/components/ListingSidebar.tsx`

**Step 1: Add props and imports**

Update the imports and props interface to include custom list functionality:

```typescript
import CustomListingInput from './CustomListingInput';

interface ListingSidebarProps {
  clusters: Cluster[];
  outliers: ClusterOutlier[];
  listings: Listing[];
  isLoading: boolean;
  totalCount: number;
  onSelect: (listing: Listing | ClusterOutlier) => void;
  onClusterClick: (cluster: Cluster) => void;
  expandedCluster: Cluster | null;
  onBack: () => void;
  onHover: (id: number | null) => void;
  onClusterHover: (id: string | null) => void;
  // New props for custom lists
  selectedListId: number | null;
  onAddListing?: (input: string) => Promise<void>;
  onRemoveListing?: (listingId: number) => void;
  addListingLoading?: boolean;
  addListingError?: string | null;
}
```

**Step 2: Add empty state and input for custom lists**

In the aggregated view section, add conditional rendering for custom list mode:

When `selectedListId !== null` and list is empty (totalCount === 0 and no clusters/outliers), show:

```tsx
{selectedListId !== null && totalCount === 0 && !isLoading && (
  <div className="flex flex-col items-center justify-center py-12 px-4 text-center">
    <div className="w-16 h-16 mb-4 text-gray-300">
      <svg fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
      </svg>
    </div>
    <h3 className="text-lg font-medium text-gray-900 mb-2">No listings yet</h3>
    <p className="text-sm text-gray-500 mb-6">
      Add listings from realtor.ca to build your custom list
    </p>
    <div className="w-full max-w-sm">
      <CustomListingInput
        onSubmit={onAddListing!}
        isLoading={addListingLoading}
        error={addListingError}
      />
    </div>
  </div>
)}
```

When `selectedListId !== null` and list has items, show input at top:

```tsx
{selectedListId !== null && totalCount > 0 && (
  <div className="mb-4">
    <CustomListingInput
      onSubmit={onAddListing!}
      isLoading={addListingLoading}
      error={addListingError}
    />
  </div>
)}
```

**Step 3: Add X button to outlier cards in custom list mode**

Modify the outlier rendering to include a remove button when in custom list mode:

```tsx
{selectedListId !== null && onRemoveListing && (
  <button
    onClick={(e) => {
      e.stopPropagation();
      onRemoveListing(outlier.id);
    }}
    className="absolute top-1 right-1 p-1 text-gray-400 hover:text-red-600 bg-white rounded-full shadow"
    title="Remove from list"
  >
    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
    </svg>
  </button>
)}
```

**Step 4: Commit**

```bash
git add frontend/src/components/ListingSidebar.tsx
git commit -m "feat: add custom list support to ListingSidebar"
```

---

## Task 13: Update Dashboard to Integrate Custom Lists

**Files:**
- Modify: `frontend/src/pages/Dashboard.tsx`

**Step 1: Add imports and hooks**

```typescript
import {
  useCustomLists,
  useCreateCustomList,
  useUpdateCustomList,
  useDeleteCustomList,
  useAddListingToCustomList,
  useRemoveListingFromCustomList,
} from '../hooks/useCustomLists';
import ListSelector from '../components/ListSelector';
```

**Step 2: Add state for selected list**

```typescript
const [selectedListId, setSelectedListId] = useState<number | null>(null);
```

**Step 3: Add hooks**

```typescript
const { data: customLists = [] } = useCustomLists();
const createListMutation = useCreateCustomList();
const updateListMutation = useUpdateCustomList();
const deleteListMutation = useDeleteCustomList();
const addListingMutation = useAddListingToCustomList();
const removeListingMutation = useRemoveListingFromCustomList();
```

**Step 4: Update filterGroups to include custom_list_id**

Modify where filterGroups is used to include the selected list:

```typescript
const effectiveFilterGroups = {
  ...filterGroups,
  custom_list_id: selectedListId ?? undefined,
};
```

Pass `effectiveFilterGroups` to `useClusters` and `useListings` instead of `filterGroups`.

**Step 5: Add handlers**

```typescript
const handleCreateList = () => {
  createListMutation.mutate(undefined);
};

const handleRenameList = (id: number, name: string) => {
  updateListMutation.mutate({ id, name });
};

const handleDeleteList = (id: number) => {
  deleteListMutation.mutate(id);
  if (selectedListId === id) {
    setSelectedListId(null);
  }
};

const handleAddListing = async (input: string) => {
  if (selectedListId === null) return;
  await addListingMutation.mutateAsync({ listId: selectedListId, input });
};

const handleRemoveListing = (listingId: number) => {
  if (selectedListId === null) return;
  removeListingMutation.mutate({ listId: selectedListId, listingId });
};
```

**Step 6: Add ListSelector to sidebar header area**

In the sidebar div, add the ListSelector component before/above the ListingSidebar:

```tsx
<div className="w-80 bg-white border-l overflow-hidden flex flex-col">
  {/* List selector header */}
  <div className="p-3 border-b bg-gray-50">
    <ListSelector
      customLists={customLists}
      selectedListId={selectedListId}
      onSelect={setSelectedListId}
      onCreateList={handleCreateList}
      onRenameList={handleRenameList}
      onDeleteList={handleDeleteList}
    />
  </div>

  {/* Sidebar content */}
  <div className="flex-1 overflow-hidden">
    {selectedListing ? (
      <ListingDetail ... />
    ) : (
      <ListingSidebar
        ...
        selectedListId={selectedListId}
        onAddListing={selectedListId ? handleAddListing : undefined}
        onRemoveListing={selectedListId ? handleRemoveListing : undefined}
        addListingLoading={addListingMutation.isPending}
        addListingError={addListingMutation.error?.message ?? null}
      />
    )}
  </div>
</div>
```

**Step 7: Commit**

```bash
git add frontend/src/pages/Dashboard.tsx
git commit -m "feat: integrate custom lists into Dashboard"
```

---

## Task 14: Update useClusters Hook

**Files:**
- Modify: `frontend/src/hooks/useClusters.ts`

**Step 1: Pass custom_list_id through to API**

The hook should already pass filterGroups to the API. Verify that the `custom_list_id` field is included in the request body.

If not, ensure the hook passes the full filterGroups object (which now includes custom_list_id) to `api.getClustersWithGroups`.

**Step 2: Commit (if changes needed)**

```bash
git add frontend/src/hooks/useClusters.ts
git commit -m "feat: ensure custom_list_id passed in cluster requests"
```

---

## Task 15: End-to-End Testing

**Step 1: Start backend**

```bash
cd backend && uvicorn app.main:app --reload
```

**Step 2: Start frontend**

```bash
cd frontend && npm run dev
```

**Step 3: Manual testing checklist**

1. [ ] Create a new custom list via "+ New" button
2. [ ] List appears in dropdown with auto-generated name
3. [ ] Select the custom list - map should be empty
4. [ ] Empty state shows with input prompt
5. [ ] Paste a realtor.ca URL and click Add
6. [ ] Listing appears on map and in sidebar
7. [ ] Add same listing again - should silently succeed (no duplicate)
8. [ ] Add a different listing by MLS number (one that exists in DB)
9. [ ] Rename the list via edit icon
10. [ ] Remove a listing via X button
11. [ ] Switch to "All Listings" - see all listings
12. [ ] Switch back to custom list - see only custom list items
13. [ ] Delete the custom list
14. [ ] Create another list, verify filtering works correctly

**Step 4: Final commit**

```bash
git add -A
git commit -m "feat: custom lists feature complete"
```

---

## Summary

This plan implements the custom lists feature in 15 tasks:

1. Database migration
2. SQLAlchemy models
3. Pydantic schemas
4. Scraper fetch_single method
5. Custom lists API endpoints
6. Listings/clusters filter integration
7. Frontend types
8. Frontend API client
9. Frontend hooks
10. ListSelector component
11. CustomListingInput component
12. ListingSidebar updates
13. Dashboard integration
14. useClusters hook updates
15. End-to-end testing

Each task is self-contained with clear steps, file paths, and code snippets.

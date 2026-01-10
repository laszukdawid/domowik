# Custom Lists Feature Design

## Overview

Add the ability for users to create multiple named lists of listings. Each list contains listings added manually via realtor.ca URL or MLS number. All users see the same lists (no per-user separation). When viewing a custom list, only those listings appear on the map.

## User Experience

### Tab/List Selection
- Dropdown selector in sidebar header: `[All Listings ▼] [+ New List]`
- Dropdown includes filter input for searching lists by name
- "All Listings" is always first, shows all scraped listings (current behavior)
- Custom lists show inline rename (✎) and delete (🗑) icons
- "+ New List" creates list with auto-generated name ("Custom Listing #N")

### Adding Listings to a List
- When custom list is empty: prominent empty state with input prompt
- When list has items: small "Add listing" input at top
- Input accepts either:
  - Full realtor.ca URL (fetches from API if not in DB)
  - MLS number (DB lookup only)
- Placeholder text: "Paste realtor.ca URL or MLS number"

### Removing Listings
- X button on each listing card when viewing a custom list
- Removes from list only (listing stays in DB and other lists)

### List Management
- Rename: click ✎ icon, edit inline or modal
- Delete: click 🗑 icon, confirms deletion (cascades to remove list entries)

## Data Model

### New table: `custom_lists`
```sql
CREATE TABLE custom_lists (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

### New table: `custom_list_listings`
```sql
CREATE TABLE custom_list_listings (
    custom_list_id INTEGER REFERENCES custom_lists(id) ON DELETE CASCADE,
    listing_id INTEGER REFERENCES listings(id) ON DELETE CASCADE,
    source_url TEXT,  -- original URL submitted (nullable for MLS-only adds)
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (custom_list_id, listing_id)
);

CREATE INDEX idx_custom_list_listings_list ON custom_list_listings(custom_list_id);
CREATE INDEX idx_custom_list_listings_listing ON custom_list_listings(listing_id);
```

## Backend API

### List Management

```
POST /api/custom-lists
  Body: { "name": "Coquitlam Listings" }  // optional, auto-generates if missing
  Response: { "id": 1, "name": "Coquitlam Listings", "created_at": "..." }

GET /api/custom-lists
  Response: [{ "id": 1, "name": "...", "count": 5, "created_at": "..." }, ...]

PATCH /api/custom-lists/{id}
  Body: { "name": "New Name" }
  Response: { "id": 1, "name": "New Name", "created_at": "..." }

DELETE /api/custom-lists/{id}
  Response: 204 No Content
  (Cascades to remove all custom_list_listings entries)
```

### Listing Management Within Lists

```
POST /api/custom-lists/{id}/listings
  Body: { "input": "https://realtor.ca/..." }  // or MLS number
  Response: { "listing_id": 123, "status": "created" | "existing" }

DELETE /api/custom-lists/{id}/listings/{listing_id}
  Response: 204 No Content
```

### Filtering Existing Endpoints

Add optional `custom_list_id` query parameter to:
- `GET /api/listings/stream`
- `POST /api/listings/stream-groups`
- `GET /api/clusters`
- `POST /api/clusters/groups`

When set, only return listings that exist in that custom list.

## Input Parsing Logic

```
1. Trim input

2. If starts with http:// or https://
   → Validate it's realtor.ca domain
   → Extract MLS ID from URL path
   → Check DB for existing listing by mls_id
   → If not in DB: fetch via RealtorCaScraper, insert listing
   → Add to custom_list_listings
   → Trigger background amenity enrichment

3. Else (treat as MLS number)
   → Validate format (numeric)
   → Check DB only
   → If found: add to custom_list_listings
   → If not found: error "Listing not found. Try pasting the full realtor.ca URL instead."
```

## Scraping Flow

### New scraper method
```python
# In RealtorCaScraper
async def fetch_single(self, mls_id: str) -> dict:
    """Fetch a single listing by MLS ID using the existing API."""
```

### Background enrichment
- Use existing AmenityEnricher logic
- Run via FastAPI BackgroundTasks after response is sent
- Frontend shows listing immediately; scores appear on next load/refresh

## Error Handling

### URL/Input Errors
- Invalid URL format → "Please enter a valid realtor.ca URL"
- Not a realtor.ca URL → "Only realtor.ca listing URLs are supported"
- Can't parse MLS ID → "Couldn't find listing ID in URL"
- Invalid MLS format → "Please enter a valid MLS number"

### Scraping Errors
- Listing not found on realtor.ca → "Listing not found - it may have been removed"
- API/network error → "Couldn't fetch listing - please try again"
- Rate limited → "Too many requests - please wait a moment"

### List Errors
- Listing already in this list → Silently succeed (idempotent)
- List not found → "List not found"

All errors shown as toast notifications.

## Frontend Components

### New Components
- `ListSelector.tsx` - Dropdown with filter input, list items, edit/delete actions
- `CustomListingInput.tsx` - URL/MLS input with validation
- `EditListNameModal.tsx` - Modal for renaming lists (or inline edit)

### Modified Components
- `Dashboard.tsx` - Add `selectedListId` state, fetch custom lists
- `ListingSidebar.tsx` - Add ListSelector in header, conditional empty state, X buttons on cards
- `useListings.ts` - Accept and forward `custom_list_id` param
- `useClusters.ts` - Accept and forward `custom_list_id` param
- `api/client.ts` - Add custom list API methods

### Map Behavior
- No direct Map.tsx changes needed
- Hooks filter by `custom_list_id` → map receives filtered listings
- Clusters also respect the filter

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

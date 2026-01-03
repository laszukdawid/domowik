from typing import Annotated
from datetime import datetime
from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from app.models import get_db, UserListingStatus, User, Listing
from app.api.deps import get_current_user

router = APIRouter(prefix="/listings/{listing_id}/status", tags=["status"])


class StatusUpdate(BaseModel):
    is_favorite: bool | None = None
    is_hidden: bool | None = None


class StatusResponse(BaseModel):
    is_favorite: bool
    is_hidden: bool
    viewed_at: datetime | None

    class Config:
        from_attributes = True


@router.get("", response_model=StatusResponse)
async def get_status(
    listing_id: int,
    db: Annotated[AsyncSession, Depends(get_db)],
    user: Annotated[User, Depends(get_current_user)],
):
    result = await db.execute(
        select(UserListingStatus).where(
            UserListingStatus.user_id == user.id,
            UserListingStatus.listing_id == listing_id,
        )
    )
    status = result.scalar_one_or_none()

    if not status:
        return StatusResponse(is_favorite=False, is_hidden=False, viewed_at=None)

    return StatusResponse.model_validate(status)


@router.put("", response_model=StatusResponse)
async def update_status(
    listing_id: int,
    req: StatusUpdate,
    db: Annotated[AsyncSession, Depends(get_db)],
    user: Annotated[User, Depends(get_current_user)],
):
    listing = await db.get(Listing, listing_id)
    if not listing:
        raise HTTPException(status_code=404, detail="Listing not found")

    result = await db.execute(
        select(UserListingStatus).where(
            UserListingStatus.user_id == user.id,
            UserListingStatus.listing_id == listing_id,
        )
    )
    status = result.scalar_one_or_none()

    if not status:
        status = UserListingStatus(
            user_id=user.id,
            listing_id=listing_id,
            is_favorite=req.is_favorite or False,
            is_hidden=req.is_hidden or False,
            viewed_at=datetime.utcnow(),
        )
        db.add(status)
    else:
        if req.is_favorite is not None:
            status.is_favorite = req.is_favorite
        if req.is_hidden is not None:
            status.is_hidden = req.is_hidden

    await db.commit()
    await db.refresh(status)

    return StatusResponse.model_validate(status)

from typing import Annotated
from fastapi import APIRouter, Depends
from pydantic import BaseModel
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from app.models import get_db, UserPreferences, User
from app.api.deps import get_current_user

router = APIRouter(prefix="/preferences", tags=["preferences"])


class PreferencesUpdate(BaseModel):
    min_price: int | None = None
    max_price: int | None = None
    min_bedrooms: int | None = None
    min_sqft: int | None = None
    cities: list[str] | None = None
    property_types: list[str] | None = None
    max_park_distance: int | None = None
    notify_email: bool | None = None


class PreferencesResponse(BaseModel):
    min_price: int | None
    max_price: int | None
    min_bedrooms: int | None
    min_sqft: int | None
    cities: list[str] | None
    property_types: list[str] | None
    max_park_distance: int | None
    notify_email: bool

    class Config:
        from_attributes = True


@router.get("", response_model=PreferencesResponse)
async def get_preferences(
    db: Annotated[AsyncSession, Depends(get_db)],
    user: Annotated[User, Depends(get_current_user)],
):
    result = await db.execute(
        select(UserPreferences).where(UserPreferences.user_id == user.id)
    )
    prefs = result.scalar_one_or_none()

    if not prefs:
        return PreferencesResponse(
            min_price=None,
            max_price=None,
            min_bedrooms=None,
            min_sqft=None,
            cities=None,
            property_types=None,
            max_park_distance=None,
            notify_email=True,
        )

    return PreferencesResponse.model_validate(prefs)


@router.put("", response_model=PreferencesResponse)
async def update_preferences(
    req: PreferencesUpdate,
    db: Annotated[AsyncSession, Depends(get_db)],
    user: Annotated[User, Depends(get_current_user)],
):
    result = await db.execute(
        select(UserPreferences).where(UserPreferences.user_id == user.id)
    )
    prefs = result.scalar_one_or_none()

    if not prefs:
        prefs = UserPreferences(user_id=user.id)
        db.add(prefs)

    for field, value in req.model_dump(exclude_unset=True).items():
        setattr(prefs, field, value)

    await db.commit()
    await db.refresh(prefs)

    return PreferencesResponse.model_validate(prefs)

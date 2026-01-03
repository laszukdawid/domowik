from typing import Annotated
from datetime import datetime
from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from app.models import get_db, UserNote, User, Listing
from app.api.deps import get_current_user

router = APIRouter(prefix="/listings/{listing_id}/notes", tags=["notes"])


class NoteCreate(BaseModel):
    note: str


class NoteResponse(BaseModel):
    id: int
    listing_id: int
    user_id: int
    user_name: str
    note: str
    created_at: datetime

    class Config:
        from_attributes = True


@router.get("", response_model=list[NoteResponse])
async def get_notes(
    listing_id: int,
    db: Annotated[AsyncSession, Depends(get_db)],
    user: Annotated[User, Depends(get_current_user)],
):
    query = (
        select(UserNote, User.name)
        .join(User)
        .where(UserNote.listing_id == listing_id)
        .order_by(UserNote.created_at.desc())
    )
    result = await db.execute(query)
    rows = result.all()

    return [
        NoteResponse(
            id=note.id,
            listing_id=note.listing_id,
            user_id=note.user_id,
            user_name=name,
            note=note.note,
            created_at=note.created_at,
        )
        for note, name in rows
    ]


@router.post("", response_model=NoteResponse)
async def create_note(
    listing_id: int,
    req: NoteCreate,
    db: Annotated[AsyncSession, Depends(get_db)],
    user: Annotated[User, Depends(get_current_user)],
):
    listing = await db.get(Listing, listing_id)
    if not listing:
        raise HTTPException(status_code=404, detail="Listing not found")

    note = UserNote(listing_id=listing_id, user_id=user.id, note=req.note)
    db.add(note)
    await db.commit()
    await db.refresh(note)

    return NoteResponse(
        id=note.id,
        listing_id=note.listing_id,
        user_id=note.user_id,
        user_name=user.name,
        note=note.note,
        created_at=note.created_at,
    )


@router.delete("/{note_id}")
async def delete_note(
    listing_id: int,
    note_id: int,
    db: Annotated[AsyncSession, Depends(get_db)],
    user: Annotated[User, Depends(get_current_user)],
):
    note = await db.get(UserNote, note_id)
    if not note or note.listing_id != listing_id:
        raise HTTPException(status_code=404, detail="Note not found")
    if note.user_id != user.id:
        raise HTTPException(status_code=403, detail="Not your note")

    await db.delete(note)
    await db.commit()
    return {"ok": True}

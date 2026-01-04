"""make all datetime columns timezone aware

Revision ID: e1d361bc305e
Revises: 19d982d9b7eb
Create Date: 2026-01-04 15:08:33.093624
"""
from typing import Sequence, Union
from alembic import op
import sqlalchemy as sa
from sqlalchemy.dialects import postgresql

revision: str = 'e1d361bc305e'
down_revision: Union[str, None] = '19d982d9b7eb'
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    # Convert remaining datetime columns to timezone-aware (TIMESTAMP WITH TIME ZONE)
    # Existing naive timestamps are interpreted as UTC
    op.alter_column('user_listing_status', 'viewed_at',
               existing_type=postgresql.TIMESTAMP(),
               type_=sa.DateTime(timezone=True),
               existing_nullable=True,
               postgresql_using="viewed_at AT TIME ZONE 'UTC'")
    op.alter_column('user_notes', 'created_at',
               existing_type=postgresql.TIMESTAMP(),
               type_=sa.DateTime(timezone=True),
               existing_nullable=False,
               postgresql_using="created_at AT TIME ZONE 'UTC'")
    op.alter_column('users', 'created_at',
               existing_type=postgresql.TIMESTAMP(),
               type_=sa.DateTime(timezone=True),
               existing_nullable=False,
               postgresql_using="created_at AT TIME ZONE 'UTC'")


def downgrade() -> None:
    op.alter_column('users', 'created_at',
               existing_type=sa.DateTime(timezone=True),
               type_=postgresql.TIMESTAMP(),
               existing_nullable=False)
    op.alter_column('user_notes', 'created_at',
               existing_type=sa.DateTime(timezone=True),
               type_=postgresql.TIMESTAMP(),
               existing_nullable=False)
    op.alter_column('user_listing_status', 'viewed_at',
               existing_type=sa.DateTime(timezone=True),
               type_=postgresql.TIMESTAMP(),
               existing_nullable=True)

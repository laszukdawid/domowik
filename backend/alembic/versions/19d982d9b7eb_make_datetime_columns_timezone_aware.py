"""make datetime columns timezone aware

Revision ID: 19d982d9b7eb
Revises: 001
Create Date: 2026-01-04 14:43:22.149011
"""
from typing import Sequence, Union
from alembic import op
import sqlalchemy as sa
from sqlalchemy.dialects import postgresql

revision: str = '19d982d9b7eb'
down_revision: Union[str, None] = '001'
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    # Convert datetime columns to timezone-aware (TIMESTAMP WITH TIME ZONE)
    # Existing naive timestamps are interpreted as UTC
    op.alter_column('listings', 'first_seen',
               existing_type=postgresql.TIMESTAMP(),
               type_=sa.DateTime(timezone=True),
               existing_nullable=False,
               postgresql_using='first_seen AT TIME ZONE \'UTC\'')
    op.alter_column('listings', 'last_seen',
               existing_type=postgresql.TIMESTAMP(),
               type_=sa.DateTime(timezone=True),
               existing_nullable=False,
               postgresql_using='last_seen AT TIME ZONE \'UTC\'')


def downgrade() -> None:
    op.alter_column('listings', 'last_seen',
               existing_type=sa.DateTime(timezone=True),
               type_=postgresql.TIMESTAMP(),
               existing_nullable=False)
    op.alter_column('listings', 'first_seen',
               existing_type=sa.DateTime(timezone=True),
               type_=postgresql.TIMESTAMP(),
               existing_nullable=False)

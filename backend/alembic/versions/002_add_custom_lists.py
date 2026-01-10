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

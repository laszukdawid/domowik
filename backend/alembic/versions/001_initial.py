"""Initial migration

Revision ID: 001
Revises:
Create Date: 2026-01-03
"""
from typing import Sequence, Union
from alembic import op
import sqlalchemy as sa
from sqlalchemy.dialects import postgresql
import geoalchemy2

revision: str = "001"
down_revision: Union[str, None] = None
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.execute("CREATE EXTENSION IF NOT EXISTS postgis")

    op.create_table(
        "users",
        sa.Column("id", sa.Integer(), nullable=False),
        sa.Column("email", sa.String(255), nullable=False),
        sa.Column("name", sa.String(100), nullable=False),
        sa.Column("password_hash", sa.String(255), nullable=False),
        sa.Column("created_at", sa.DateTime(), nullable=False),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index("ix_users_email", "users", ["email"], unique=True)

    op.create_table(
        "user_preferences",
        sa.Column("user_id", sa.Integer(), nullable=False),
        sa.Column("min_price", sa.Integer(), nullable=True),
        sa.Column("max_price", sa.Integer(), nullable=True),
        sa.Column("min_bedrooms", sa.Integer(), nullable=True),
        sa.Column("min_sqft", sa.Integer(), nullable=True),
        sa.Column("cities", postgresql.ARRAY(sa.String()), nullable=True),
        sa.Column("property_types", postgresql.ARRAY(sa.String()), nullable=True),
        sa.Column("max_park_distance", sa.Integer(), nullable=True),
        sa.Column("notify_email", sa.Boolean(), nullable=False, server_default="true"),
        sa.ForeignKeyConstraint(["user_id"], ["users.id"]),
        sa.PrimaryKeyConstraint("user_id"),
    )

    op.create_table(
        "listings",
        sa.Column("id", sa.Integer(), nullable=False),
        sa.Column("mls_id", sa.String(50), nullable=False),
        sa.Column("url", sa.String(500), nullable=False),
        sa.Column("address", sa.String(500), nullable=False),
        sa.Column("city", sa.String(100), nullable=False),
        sa.Column(
            "location", geoalchemy2.Geometry("POINT", srid=4326), nullable=True
        ),
        sa.Column("price", sa.Integer(), nullable=False),
        sa.Column("bedrooms", sa.Integer(), nullable=True),
        sa.Column("bathrooms", sa.Integer(), nullable=True),
        sa.Column("sqft", sa.Integer(), nullable=True),
        sa.Column("property_type", sa.String(50), nullable=True),
        sa.Column("listing_date", sa.Date(), nullable=True),
        sa.Column("first_seen", sa.DateTime(), nullable=False),
        sa.Column("last_seen", sa.DateTime(), nullable=False),
        sa.Column("status", sa.String(20), nullable=False, server_default="active"),
        sa.Column("raw_data", postgresql.JSONB(), nullable=True),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index("ix_listings_mls_id", "listings", ["mls_id"], unique=True)
    op.create_index("ix_listings_city", "listings", ["city"])
    op.create_index("ix_listings_price", "listings", ["price"])
    op.create_index("ix_listings_status", "listings", ["status"])
    op.create_index(
        "ix_listings_location", "listings", ["location"], postgresql_using="gist"
    )

    op.create_table(
        "amenity_scores",
        sa.Column("listing_id", sa.Integer(), nullable=False),
        sa.Column("nearest_park_m", sa.Integer(), nullable=True),
        sa.Column("nearest_coffee_m", sa.Integer(), nullable=True),
        sa.Column("nearest_dog_park_m", sa.Integer(), nullable=True),
        sa.Column("parks", postgresql.JSONB(), nullable=True),
        sa.Column("coffee_shops", postgresql.JSONB(), nullable=True),
        sa.Column("walkability_score", sa.Integer(), nullable=True),
        sa.Column("amenity_score", sa.Integer(), nullable=True),
        sa.ForeignKeyConstraint(["listing_id"], ["listings.id"]),
        sa.PrimaryKeyConstraint("listing_id"),
    )

    op.create_table(
        "user_notes",
        sa.Column("id", sa.Integer(), nullable=False),
        sa.Column("listing_id", sa.Integer(), nullable=False),
        sa.Column("user_id", sa.Integer(), nullable=False),
        sa.Column("note", sa.Text(), nullable=False),
        sa.Column("created_at", sa.DateTime(), nullable=False),
        sa.ForeignKeyConstraint(["listing_id"], ["listings.id"]),
        sa.ForeignKeyConstraint(["user_id"], ["users.id"]),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index("ix_user_notes_listing_id", "user_notes", ["listing_id"])
    op.create_index("ix_user_notes_user_id", "user_notes", ["user_id"])

    op.create_table(
        "user_listing_status",
        sa.Column("user_id", sa.Integer(), nullable=False),
        sa.Column("listing_id", sa.Integer(), nullable=False),
        sa.Column("is_favorite", sa.Boolean(), nullable=False, server_default="false"),
        sa.Column("is_hidden", sa.Boolean(), nullable=False, server_default="false"),
        sa.Column("viewed_at", sa.DateTime(), nullable=True),
        sa.ForeignKeyConstraint(["listing_id"], ["listings.id"]),
        sa.ForeignKeyConstraint(["user_id"], ["users.id"]),
        sa.PrimaryKeyConstraint("user_id", "listing_id"),
    )


def downgrade() -> None:
    op.drop_table("user_listing_status")
    op.drop_table("user_notes")
    op.drop_table("amenity_scores")
    op.drop_table("listings")
    op.drop_table("user_preferences")
    op.drop_table("users")

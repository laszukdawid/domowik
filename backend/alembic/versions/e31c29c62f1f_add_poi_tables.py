"""add_poi_tables

Revision ID: e31c29c62f1f
Revises: e1d361bc305e
Create Date: 2026-01-06 06:15:34.024275
"""
from typing import Sequence, Union
from alembic import op
import sqlalchemy as sa
import geoalchemy2


revision: str = 'e31c29c62f1f'
down_revision: Union[str, None] = 'e1d361bc305e'
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.create_table(
        "points_of_interest",
        sa.Column("id", sa.Integer(), nullable=False),
        sa.Column("osm_id", sa.BigInteger(), nullable=False),
        sa.Column("type", sa.String(50), nullable=False),
        sa.Column("name", sa.String(255), nullable=True),
        sa.Column("geometry", geoalchemy2.Geometry(srid=4326), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.func.now()),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index("ix_poi_osm_id", "points_of_interest", ["osm_id"], unique=True)
    op.create_index("ix_poi_type", "points_of_interest", ["type"])
    op.create_index("ix_poi_geometry", "points_of_interest", ["geometry"], postgresql_using="gist")

    op.create_table(
        "listing_pois",
        sa.Column("listing_id", sa.Integer(), nullable=False),
        sa.Column("poi_id", sa.Integer(), nullable=False),
        sa.Column("distance_m", sa.Integer(), nullable=False),
        sa.ForeignKeyConstraint(["listing_id"], ["listings.id"], ondelete="CASCADE"),
        sa.ForeignKeyConstraint(["poi_id"], ["points_of_interest.id"], ondelete="CASCADE"),
        sa.PrimaryKeyConstraint("listing_id", "poi_id"),
    )
    op.create_index("ix_listing_pois_listing", "listing_pois", ["listing_id"])
    op.create_index("ix_listing_pois_poi", "listing_pois", ["poi_id"])


def downgrade() -> None:
    op.drop_table("listing_pois")
    op.drop_table("points_of_interest")

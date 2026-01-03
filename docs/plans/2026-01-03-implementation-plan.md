# HomeHero Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a house search app that scrapes Vancouver MLS listings, enriches with amenity data, and displays on an interactive map with email notifications.

**Architecture:** FastAPI backend with PostgreSQL+PostGIS, React+Leaflet frontend, Python scraper as scheduled job. JWT auth for multi-user support.

**Tech Stack:** Python 3.12, FastAPI, SQLAlchemy, PostgreSQL+PostGIS, React 18, TypeScript, Leaflet, Tailwind, Docker

---

## Phase 1: Project Setup & Infrastructure

### Task 1.1: Create Project Structure

**Files:**
- Create: `backend/app/__init__.py`
- Create: `backend/app/main.py`
- Create: `backend/app/config.py`
- Create: `backend/requirements.txt`
- Create: `frontend/package.json`
- Create: `docker-compose.yml`
- Create: `.env.example`
- Create: `.gitignore`

**Step 1: Create backend directory structure**

```bash
mkdir -p backend/app/{api,models,services}
mkdir -p backend/scraper
mkdir -p backend/alembic
touch backend/app/__init__.py
touch backend/app/api/__init__.py
touch backend/app/models/__init__.py
touch backend/app/services/__init__.py
```

**Step 2: Create backend/requirements.txt**

```
fastapi==0.109.0
uvicorn[standard]==0.27.0
sqlalchemy[asyncio]==2.0.25
asyncpg==0.29.0
geoalchemy2==0.14.3
alembic==1.13.1
pydantic==2.5.3
pydantic-settings==2.1.0
python-jose[cryptography]==3.3.0
passlib[bcrypt]==1.7.4
httpx==0.26.0
jinja2==3.1.3
python-multipart==0.0.6
```

**Step 3: Create backend/app/config.py**

```python
from pydantic_settings import BaseSettings


class Settings(BaseSettings):
    database_url: str = "postgresql+asyncpg://postgres:postgres@db:5432/housesearch"
    jwt_secret: str = "change-me-in-production"
    jwt_algorithm: str = "HS256"
    jwt_expire_minutes: int = 60 * 24 * 7  # 1 week

    smtp_host: str = "mail.homehero.pro"
    smtp_port: int = 587
    smtp_user: str = "notifications@homehero.pro"
    smtp_pass: str = ""

    class Config:
        env_file = ".env"


settings = Settings()
```

**Step 4: Create backend/app/main.py**

```python
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

app = FastAPI(title="HomeHero API", version="0.1.0")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:3000"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


@app.get("/health")
async def health():
    return {"status": "ok"}
```

**Step 5: Create docker-compose.yml**

```yaml
services:
  db:
    image: postgis/postgis:16-3.4-alpine
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: housesearch
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data

  backend:
    build: ./backend
    ports:
      - "8000:8000"
    environment:
      DATABASE_URL: postgresql+asyncpg://postgres:postgres@db:5432/housesearch
    depends_on:
      - db
    volumes:
      - ./backend:/app
    command: uvicorn app.main:app --host 0.0.0.0 --reload

  frontend:
    build: ./frontend
    ports:
      - "3000:3000"
    volumes:
      - ./frontend:/app
      - /app/node_modules
    environment:
      - VITE_API_URL=http://localhost:8000

volumes:
  postgres_data:
```

**Step 6: Create backend/Dockerfile**

```dockerfile
FROM python:3.12-slim

WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY . .

CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0"]
```

**Step 7: Create .env.example**

```
DATABASE_URL=postgresql+asyncpg://postgres:postgres@db:5432/housesearch
JWT_SECRET=change-me-in-production
SMTP_HOST=mail.homehero.pro
SMTP_USER=notifications@homehero.pro
SMTP_PASS=
```

**Step 8: Create .gitignore**

```
__pycache__/
*.pyc
.env
node_modules/
dist/
.venv/
*.egg-info/
.pytest_cache/
```

**Step 9: Commit**

```bash
git add -A
git commit -m "feat: initialize project structure with Docker Compose"
```

---

### Task 1.2: Create Database Models

**Files:**
- Create: `backend/app/models/base.py`
- Create: `backend/app/models/user.py`
- Create: `backend/app/models/listing.py`
- Create: `backend/app/models/amenity.py`
- Create: `backend/app/models/note.py`

**Step 1: Create backend/app/models/base.py**

```python
from sqlalchemy.ext.asyncio import AsyncAttrs, create_async_engine, async_sessionmaker
from sqlalchemy.orm import DeclarativeBase

from app.config import settings

engine = create_async_engine(settings.database_url, echo=True)
async_session = async_sessionmaker(engine, expire_on_commit=False)


class Base(AsyncAttrs, DeclarativeBase):
    pass


async def get_db():
    async with async_session() as session:
        yield session
```

**Step 2: Create backend/app/models/user.py**

```python
from datetime import datetime
from sqlalchemy import String, Boolean, DateTime, Integer, ARRAY
from sqlalchemy.orm import Mapped, mapped_column, relationship
from sqlalchemy.dialects.postgresql import JSONB

from app.models.base import Base


class User(Base):
    __tablename__ = "users"

    id: Mapped[int] = mapped_column(primary_key=True)
    email: Mapped[str] = mapped_column(String(255), unique=True, index=True)
    name: Mapped[str] = mapped_column(String(100))
    password_hash: Mapped[str] = mapped_column(String(255))
    created_at: Mapped[datetime] = mapped_column(DateTime, default=datetime.utcnow)

    preferences: Mapped["UserPreferences"] = relationship(back_populates="user", uselist=False)
    notes: Mapped[list["UserNote"]] = relationship(back_populates="user")
    listing_statuses: Mapped[list["UserListingStatus"]] = relationship(back_populates="user")


class UserPreferences(Base):
    __tablename__ = "user_preferences"

    user_id: Mapped[int] = mapped_column(Integer, primary_key=True)
    min_price: Mapped[int | None] = mapped_column(Integer, nullable=True)
    max_price: Mapped[int | None] = mapped_column(Integer, nullable=True)
    min_bedrooms: Mapped[int | None] = mapped_column(Integer, nullable=True)
    min_sqft: Mapped[int | None] = mapped_column(Integer, nullable=True)
    cities: Mapped[list[str] | None] = mapped_column(ARRAY(String), nullable=True)
    property_types: Mapped[list[str] | None] = mapped_column(ARRAY(String), nullable=True)
    max_park_distance: Mapped[int | None] = mapped_column(Integer, nullable=True)
    notify_email: Mapped[bool] = mapped_column(Boolean, default=True)

    user: Mapped["User"] = relationship(back_populates="preferences")
```

**Step 3: Create backend/app/models/listing.py**

```python
from datetime import datetime, date
from sqlalchemy import String, Integer, DateTime, Date
from sqlalchemy.orm import Mapped, mapped_column, relationship
from sqlalchemy.dialects.postgresql import JSONB
from geoalchemy2 import Geometry

from app.models.base import Base


class Listing(Base):
    __tablename__ = "listings"

    id: Mapped[int] = mapped_column(primary_key=True)
    mls_id: Mapped[str] = mapped_column(String(50), unique=True, index=True)
    url: Mapped[str] = mapped_column(String(500))
    address: Mapped[str] = mapped_column(String(500))
    city: Mapped[str] = mapped_column(String(100), index=True)
    location = mapped_column(Geometry("POINT", srid=4326))
    price: Mapped[int] = mapped_column(Integer, index=True)
    bedrooms: Mapped[int | None] = mapped_column(Integer, nullable=True)
    bathrooms: Mapped[int | None] = mapped_column(Integer, nullable=True)
    sqft: Mapped[int | None] = mapped_column(Integer, nullable=True)
    property_type: Mapped[str | None] = mapped_column(String(50), nullable=True)
    listing_date: Mapped[date | None] = mapped_column(Date, nullable=True)
    first_seen: Mapped[datetime] = mapped_column(DateTime, default=datetime.utcnow)
    last_seen: Mapped[datetime] = mapped_column(DateTime, default=datetime.utcnow)
    status: Mapped[str] = mapped_column(String(20), default="active", index=True)
    raw_data: Mapped[dict | None] = mapped_column(JSONB, nullable=True)

    amenity_score: Mapped["AmenityScore"] = relationship(back_populates="listing", uselist=False)
    notes: Mapped[list["UserNote"]] = relationship(back_populates="listing")
    user_statuses: Mapped[list["UserListingStatus"]] = relationship(back_populates="listing")
```

**Step 4: Create backend/app/models/amenity.py**

```python
from sqlalchemy import Integer, ForeignKey
from sqlalchemy.orm import Mapped, mapped_column, relationship
from sqlalchemy.dialects.postgresql import JSONB

from app.models.base import Base


class AmenityScore(Base):
    __tablename__ = "amenity_scores"

    listing_id: Mapped[int] = mapped_column(ForeignKey("listings.id"), primary_key=True)
    nearest_park_m: Mapped[int | None] = mapped_column(Integer, nullable=True)
    nearest_coffee_m: Mapped[int | None] = mapped_column(Integer, nullable=True)
    nearest_dog_park_m: Mapped[int | None] = mapped_column(Integer, nullable=True)
    parks: Mapped[dict | None] = mapped_column(JSONB, nullable=True)
    coffee_shops: Mapped[dict | None] = mapped_column(JSONB, nullable=True)
    walkability_score: Mapped[int | None] = mapped_column(Integer, nullable=True)
    amenity_score: Mapped[int | None] = mapped_column(Integer, nullable=True)

    listing: Mapped["Listing"] = relationship(back_populates="amenity_score")
```

**Step 5: Create backend/app/models/note.py**

```python
from datetime import datetime
from sqlalchemy import Integer, ForeignKey, Text, DateTime, Boolean
from sqlalchemy.orm import Mapped, mapped_column, relationship

from app.models.base import Base


class UserNote(Base):
    __tablename__ = "user_notes"

    id: Mapped[int] = mapped_column(primary_key=True)
    listing_id: Mapped[int] = mapped_column(ForeignKey("listings.id"), index=True)
    user_id: Mapped[int] = mapped_column(ForeignKey("users.id"), index=True)
    note: Mapped[str] = mapped_column(Text)
    created_at: Mapped[datetime] = mapped_column(DateTime, default=datetime.utcnow)

    listing: Mapped["Listing"] = relationship(back_populates="notes")
    user: Mapped["User"] = relationship(back_populates="notes")


class UserListingStatus(Base):
    __tablename__ = "user_listing_status"

    user_id: Mapped[int] = mapped_column(ForeignKey("users.id"), primary_key=True)
    listing_id: Mapped[int] = mapped_column(ForeignKey("listings.id"), primary_key=True)
    is_favorite: Mapped[bool] = mapped_column(Boolean, default=False)
    is_hidden: Mapped[bool] = mapped_column(Boolean, default=False)
    viewed_at: Mapped[datetime | None] = mapped_column(DateTime, nullable=True)

    user: Mapped["User"] = relationship(back_populates="listing_statuses")
    listing: Mapped["Listing"] = relationship(back_populates="user_statuses")
```

**Step 6: Update backend/app/models/__init__.py**

```python
from app.models.base import Base, engine, async_session, get_db
from app.models.user import User, UserPreferences
from app.models.listing import Listing
from app.models.amenity import AmenityScore
from app.models.note import UserNote, UserListingStatus

__all__ = [
    "Base",
    "engine",
    "async_session",
    "get_db",
    "User",
    "UserPreferences",
    "Listing",
    "AmenityScore",
    "UserNote",
    "UserListingStatus",
]
```

**Step 7: Commit**

```bash
git add -A
git commit -m "feat: add SQLAlchemy models for users, listings, amenities, notes"
```

---

### Task 1.3: Setup Alembic Migrations

**Files:**
- Create: `backend/alembic.ini`
- Create: `backend/alembic/env.py`
- Create: `backend/alembic/script.py.mako`
- Create: `backend/alembic/versions/001_initial.py`

**Step 1: Create backend/alembic.ini**

```ini
[alembic]
script_location = alembic
prepend_sys_path = .
sqlalchemy.url = postgresql+asyncpg://postgres:postgres@db:5432/housesearch

[loggers]
keys = root,sqlalchemy,alembic

[handlers]
keys = console

[formatters]
keys = generic

[logger_root]
level = WARN
handlers = console

[logger_sqlalchemy]
level = WARN
handlers =
qualname = sqlalchemy.engine

[logger_alembic]
level = INFO
handlers =
qualname = alembic

[handler_console]
class = StreamHandler
args = (sys.stderr,)
level = NOTSET
formatter = generic

[formatter_generic]
format = %(levelname)-5.5s [%(name)s] %(message)s
```

**Step 2: Create backend/alembic/env.py**

```python
import asyncio
from logging.config import fileConfig

from sqlalchemy import pool
from sqlalchemy.ext.asyncio import async_engine_from_config
from alembic import context

from app.models import Base
from app.config import settings

config = context.config
config.set_main_option("sqlalchemy.url", settings.database_url)

if config.config_file_name is not None:
    fileConfig(config.config_file_name)

target_metadata = Base.metadata


def run_migrations_offline() -> None:
    url = config.get_main_option("sqlalchemy.url")
    context.configure(
        url=url,
        target_metadata=target_metadata,
        literal_binds=True,
        dialect_opts={"paramstyle": "named"},
    )
    with context.begin_transaction():
        context.run_migrations()


def do_run_migrations(connection):
    context.configure(connection=connection, target_metadata=target_metadata)
    with context.begin_transaction():
        context.run_migrations()


async def run_async_migrations():
    connectable = async_engine_from_config(
        config.get_section(config.config_ini_section, {}),
        prefix="sqlalchemy.",
        poolclass=pool.NullPool,
    )
    async with connectable.connect() as connection:
        await connection.run_sync(do_run_migrations)
    await connectable.dispose()


def run_migrations_online() -> None:
    asyncio.run(run_async_migrations())


if context.is_offline_mode():
    run_migrations_offline()
else:
    run_migrations_online()
```

**Step 3: Create backend/alembic/script.py.mako**

```mako
"""${message}

Revision ID: ${up_revision}
Revises: ${down_revision | comma,n}
Create Date: ${create_date}
"""
from typing import Sequence, Union
from alembic import op
import sqlalchemy as sa
import geoalchemy2
${imports if imports else ""}

revision: str = ${repr(up_revision)}
down_revision: Union[str, None] = ${repr(down_revision)}
branch_labels: Union[str, Sequence[str], None] = ${repr(branch_labels)}
depends_on: Union[str, Sequence[str], None] = ${repr(depends_on)}


def upgrade() -> None:
    ${upgrades if upgrades else "pass"}


def downgrade() -> None:
    ${downgrades if downgrades else "pass"}
```

**Step 4: Create backend/alembic/versions directory**

```bash
mkdir -p backend/alembic/versions
```

**Step 5: Create backend/alembic/versions/001_initial.py**

```python
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
        sa.Column("notify_email", sa.Boolean(), nullable=False, default=True),
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
        sa.Column("location", geoalchemy2.Geometry("POINT", srid=4326), nullable=True),
        sa.Column("price", sa.Integer(), nullable=False),
        sa.Column("bedrooms", sa.Integer(), nullable=True),
        sa.Column("bathrooms", sa.Integer(), nullable=True),
        sa.Column("sqft", sa.Integer(), nullable=True),
        sa.Column("property_type", sa.String(50), nullable=True),
        sa.Column("listing_date", sa.Date(), nullable=True),
        sa.Column("first_seen", sa.DateTime(), nullable=False),
        sa.Column("last_seen", sa.DateTime(), nullable=False),
        sa.Column("status", sa.String(20), nullable=False, default="active"),
        sa.Column("raw_data", postgresql.JSONB(), nullable=True),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index("ix_listings_mls_id", "listings", ["mls_id"], unique=True)
    op.create_index("ix_listings_city", "listings", ["city"])
    op.create_index("ix_listings_price", "listings", ["price"])
    op.create_index("ix_listings_status", "listings", ["status"])
    op.create_index("ix_listings_location", "listings", ["location"], postgresql_using="gist")

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
        sa.Column("is_favorite", sa.Boolean(), nullable=False, default=False),
        sa.Column("is_hidden", sa.Boolean(), nullable=False, default=False),
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
```

**Step 6: Commit**

```bash
git add -A
git commit -m "feat: add Alembic migrations for database schema"
```

---

## Phase 2: Backend API

### Task 2.1: Authentication API

**Files:**
- Create: `backend/app/services/auth.py`
- Create: `backend/app/api/auth.py`
- Create: `backend/app/api/deps.py`

**Step 1: Create backend/app/services/auth.py**

```python
from datetime import datetime, timedelta
from jose import jwt, JWTError
from passlib.context import CryptContext
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from app.config import settings
from app.models import User, UserPreferences

pwd_context = CryptContext(schemes=["bcrypt"], deprecated="auto")


def hash_password(password: str) -> str:
    return pwd_context.hash(password)


def verify_password(plain: str, hashed: str) -> bool:
    return pwd_context.verify(plain, hashed)


def create_token(user_id: int) -> str:
    expire = datetime.utcnow() + timedelta(minutes=settings.jwt_expire_minutes)
    return jwt.encode(
        {"sub": str(user_id), "exp": expire},
        settings.jwt_secret,
        algorithm=settings.jwt_algorithm,
    )


def decode_token(token: str) -> int | None:
    try:
        payload = jwt.decode(token, settings.jwt_secret, algorithms=[settings.jwt_algorithm])
        return int(payload.get("sub"))
    except JWTError:
        return None


async def get_user_by_email(db: AsyncSession, email: str) -> User | None:
    result = await db.execute(select(User).where(User.email == email))
    return result.scalar_one_or_none()


async def create_user(db: AsyncSession, email: str, name: str, password: str) -> User:
    user = User(email=email, name=name, password_hash=hash_password(password))
    db.add(user)
    await db.flush()

    prefs = UserPreferences(user_id=user.id)
    db.add(prefs)
    await db.commit()
    await db.refresh(user)
    return user
```

**Step 2: Create backend/app/api/deps.py**

```python
from typing import Annotated
from fastapi import Depends, HTTPException, status
from fastapi.security import HTTPBearer, HTTPAuthorizationCredentials
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from app.models import get_db, User
from app.services.auth import decode_token

security = HTTPBearer()


async def get_current_user(
    db: Annotated[AsyncSession, Depends(get_db)],
    credentials: Annotated[HTTPAuthorizationCredentials, Depends(security)],
) -> User:
    user_id = decode_token(credentials.credentials)
    if not user_id:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid token")

    result = await db.execute(select(User).where(User.id == user_id))
    user = result.scalar_one_or_none()
    if not user:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="User not found")
    return user
```

**Step 3: Create backend/app/api/auth.py**

```python
from typing import Annotated
from fastapi import APIRouter, Depends, HTTPException, status
from pydantic import BaseModel, EmailStr
from sqlalchemy.ext.asyncio import AsyncSession

from app.models import get_db
from app.services.auth import (
    get_user_by_email,
    create_user,
    verify_password,
    create_token,
)

router = APIRouter(prefix="/auth", tags=["auth"])


class RegisterRequest(BaseModel):
    email: EmailStr
    name: str
    password: str


class LoginRequest(BaseModel):
    email: EmailStr
    password: str


class TokenResponse(BaseModel):
    access_token: str
    token_type: str = "bearer"


class UserResponse(BaseModel):
    id: int
    email: str
    name: str

    class Config:
        from_attributes = True


@router.post("/register", response_model=TokenResponse)
async def register(req: RegisterRequest, db: Annotated[AsyncSession, Depends(get_db)]):
    existing = await get_user_by_email(db, req.email)
    if existing:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Email already registered")

    user = await create_user(db, req.email, req.name, req.password)
    return TokenResponse(access_token=create_token(user.id))


@router.post("/login", response_model=TokenResponse)
async def login(req: LoginRequest, db: Annotated[AsyncSession, Depends(get_db)]):
    user = await get_user_by_email(db, req.email)
    if not user or not verify_password(req.password, user.password_hash):
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid credentials")

    return TokenResponse(access_token=create_token(user.id))
```

**Step 4: Update backend/app/api/__init__.py**

```python
from app.api.auth import router as auth_router

__all__ = ["auth_router"]
```

**Step 5: Update backend/app/main.py to include router**

```python
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from app.api import auth_router

app = FastAPI(title="HomeHero API", version="0.1.0")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:3000"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(auth_router, prefix="/api")


@app.get("/health")
async def health():
    return {"status": "ok"}
```

**Step 6: Commit**

```bash
git add -A
git commit -m "feat: add authentication API with register and login"
```

---

### Task 2.2: Listings API

**Files:**
- Create: `backend/app/api/listings.py`
- Create: `backend/app/schemas/listing.py`

**Step 1: Create backend/app/schemas/__init__.py and listing.py**

```bash
mkdir -p backend/app/schemas
touch backend/app/schemas/__init__.py
```

```python
# backend/app/schemas/listing.py
from datetime import datetime, date
from pydantic import BaseModel


class AmenityScoreResponse(BaseModel):
    nearest_park_m: int | None
    nearest_coffee_m: int | None
    nearest_dog_park_m: int | None
    parks: list[dict] | None
    coffee_shops: list[dict] | None
    walkability_score: int | None
    amenity_score: int | None

    class Config:
        from_attributes = True


class ListingResponse(BaseModel):
    id: int
    mls_id: str
    url: str
    address: str
    city: str
    latitude: float | None
    longitude: float | None
    price: int
    bedrooms: int | None
    bathrooms: int | None
    sqft: int | None
    property_type: str | None
    listing_date: date | None
    first_seen: datetime
    status: str
    amenity_score: AmenityScoreResponse | None = None
    is_favorite: bool = False
    is_hidden: bool = False
    is_new: bool = False

    class Config:
        from_attributes = True


class ListingFilters(BaseModel):
    min_price: int | None = None
    max_price: int | None = None
    min_bedrooms: int | None = None
    min_sqft: int | None = None
    cities: list[str] | None = None
    property_types: list[str] | None = None
    max_park_distance: int | None = None
    include_hidden: bool = False
    favorites_only: bool = False
```

**Step 2: Create backend/app/api/listings.py**

```python
from typing import Annotated
from datetime import datetime, timedelta
from fastapi import APIRouter, Depends, Query
from sqlalchemy import select, and_
from sqlalchemy.orm import selectinload
from sqlalchemy.ext.asyncio import AsyncSession
from geoalchemy2.functions import ST_X, ST_Y

from app.models import get_db, Listing, AmenityScore, UserListingStatus, User
from app.api.deps import get_current_user
from app.schemas.listing import ListingResponse, ListingFilters, AmenityScoreResponse

router = APIRouter(prefix="/listings", tags=["listings"])


def listing_to_response(
    listing: Listing,
    user_status: UserListingStatus | None,
    last_visit: datetime | None,
) -> ListingResponse:
    amenity = None
    if listing.amenity_score:
        amenity = AmenityScoreResponse.model_validate(listing.amenity_score)

    return ListingResponse(
        id=listing.id,
        mls_id=listing.mls_id,
        url=listing.url,
        address=listing.address,
        city=listing.city,
        latitude=None,  # Will be set from geometry
        longitude=None,
        price=listing.price,
        bedrooms=listing.bedrooms,
        bathrooms=listing.bathrooms,
        sqft=listing.sqft,
        property_type=listing.property_type,
        listing_date=listing.listing_date,
        first_seen=listing.first_seen,
        status=listing.status,
        amenity_score=amenity,
        is_favorite=user_status.is_favorite if user_status else False,
        is_hidden=user_status.is_hidden if user_status else False,
        is_new=last_visit is not None and listing.first_seen > last_visit,
    )


@router.get("", response_model=list[ListingResponse])
async def get_listings(
    db: Annotated[AsyncSession, Depends(get_db)],
    user: Annotated[User, Depends(get_current_user)],
    min_price: int | None = None,
    max_price: int | None = None,
    min_bedrooms: int | None = None,
    min_sqft: int | None = None,
    cities: list[str] | None = Query(None),
    property_types: list[str] | None = Query(None),
    include_hidden: bool = False,
    favorites_only: bool = False,
):
    query = (
        select(Listing, UserListingStatus, ST_X(Listing.location), ST_Y(Listing.location))
        .outerjoin(
            UserListingStatus,
            and_(
                UserListingStatus.listing_id == Listing.id,
                UserListingStatus.user_id == user.id,
            ),
        )
        .options(selectinload(Listing.amenity_score))
        .where(Listing.status == "active")
    )

    if min_price:
        query = query.where(Listing.price >= min_price)
    if max_price:
        query = query.where(Listing.price <= max_price)
    if min_bedrooms:
        query = query.where(Listing.bedrooms >= min_bedrooms)
    if min_sqft:
        query = query.where(Listing.sqft >= min_sqft)
    if cities:
        query = query.where(Listing.city.in_(cities))
    if property_types:
        query = query.where(Listing.property_type.in_(property_types))
    if not include_hidden:
        query = query.where(
            (UserListingStatus.is_hidden == False) | (UserListingStatus.is_hidden == None)
        )
    if favorites_only:
        query = query.where(UserListingStatus.is_favorite == True)

    query = query.order_by(Listing.first_seen.desc())

    result = await db.execute(query)
    rows = result.all()

    # Get user's last visit time (simplified - would track properly in production)
    last_visit = datetime.utcnow() - timedelta(days=1)

    listings = []
    for listing, status, lng, lat in rows:
        resp = listing_to_response(listing, status, last_visit)
        resp.longitude = lng
        resp.latitude = lat
        listings.append(resp)

    return listings


@router.get("/{listing_id}", response_model=ListingResponse)
async def get_listing(
    listing_id: int,
    db: Annotated[AsyncSession, Depends(get_db)],
    user: Annotated[User, Depends(get_current_user)],
):
    query = (
        select(Listing, UserListingStatus, ST_X(Listing.location), ST_Y(Listing.location))
        .outerjoin(
            UserListingStatus,
            and_(
                UserListingStatus.listing_id == Listing.id,
                UserListingStatus.user_id == user.id,
            ),
        )
        .options(selectinload(Listing.amenity_score))
        .where(Listing.id == listing_id)
    )

    result = await db.execute(query)
    row = result.first()

    if not row:
        raise HTTPException(status_code=404, detail="Listing not found")

    listing, status, lng, lat = row
    resp = listing_to_response(listing, status, None)
    resp.longitude = lng
    resp.latitude = lat
    return resp
```

**Step 3: Update backend/app/api/__init__.py**

```python
from app.api.auth import router as auth_router
from app.api.listings import router as listings_router

__all__ = ["auth_router", "listings_router"]
```

**Step 4: Update backend/app/main.py**

```python
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from app.api import auth_router, listings_router

app = FastAPI(title="HomeHero API", version="0.1.0")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:3000"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(auth_router, prefix="/api")
app.include_router(listings_router, prefix="/api")


@app.get("/health")
async def health():
    return {"status": "ok"}
```

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: add listings API with filtering"
```

---

### Task 2.3: Notes and Status API

**Files:**
- Create: `backend/app/api/notes.py`
- Create: `backend/app/api/status.py`

**Step 1: Create backend/app/api/notes.py**

```python
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
    # Verify listing exists
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
```

**Step 2: Create backend/app/api/status.py**

```python
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
    # Verify listing exists
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
```

**Step 3: Update backend/app/api/__init__.py**

```python
from app.api.auth import router as auth_router
from app.api.listings import router as listings_router
from app.api.notes import router as notes_router
from app.api.status import router as status_router

__all__ = ["auth_router", "listings_router", "notes_router", "status_router"]
```

**Step 4: Update backend/app/main.py**

```python
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from app.api import auth_router, listings_router, notes_router, status_router

app = FastAPI(title="HomeHero API", version="0.1.0")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:3000"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(auth_router, prefix="/api")
app.include_router(listings_router, prefix="/api")
app.include_router(notes_router, prefix="/api")
app.include_router(status_router, prefix="/api")


@app.get("/health")
async def health():
    return {"status": "ok"}
```

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: add notes and listing status APIs"
```

---

### Task 2.4: User Preferences API

**Files:**
- Create: `backend/app/api/preferences.py`

**Step 1: Create backend/app/api/preferences.py**

```python
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
```

**Step 2: Update backend/app/api/__init__.py**

```python
from app.api.auth import router as auth_router
from app.api.listings import router as listings_router
from app.api.notes import router as notes_router
from app.api.status import router as status_router
from app.api.preferences import router as preferences_router

__all__ = [
    "auth_router",
    "listings_router",
    "notes_router",
    "status_router",
    "preferences_router",
]
```

**Step 3: Update backend/app/main.py**

```python
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from app.api import (
    auth_router,
    listings_router,
    notes_router,
    status_router,
    preferences_router,
)

app = FastAPI(title="HomeHero API", version="0.1.0")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:3000"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(auth_router, prefix="/api")
app.include_router(listings_router, prefix="/api")
app.include_router(notes_router, prefix="/api")
app.include_router(status_router, prefix="/api")
app.include_router(preferences_router, prefix="/api")


@app.get("/health")
async def health():
    return {"status": "ok"}
```

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: add user preferences API"
```

---

## Phase 3: Scraper & Enrichment

### Task 3.1: Realtor.ca Scraper

**Files:**
- Create: `backend/scraper/realtor_ca.py`
- Create: `backend/scraper/run.py`

(See implementation-plan-phase3.md for details)

---

## Phase 4: Frontend

### Task 4.1: React Project Setup

**Files:**
- Create: `frontend/package.json`
- Create: `frontend/vite.config.ts`
- Create: `frontend/tsconfig.json`
- Create: `frontend/index.html`
- Create: `frontend/src/main.tsx`

(See implementation-plan-phase4.md for details)

---

## Phase 5: Email Notifications

(See implementation-plan-phase5.md for details)

---

## Phase 6: Kubernetes Deployment

(See implementation-plan-phase6.md for details)

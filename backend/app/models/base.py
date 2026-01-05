from sqlalchemy.ext.asyncio import AsyncAttrs, create_async_engine, async_sessionmaker
from sqlalchemy.orm import DeclarativeBase

from app.config import settings

engine = create_async_engine(
    settings.database_url,
    echo=False,
    pool_size=20,
    max_overflow=30,
    pool_timeout=60,
)
async_session = async_sessionmaker(engine, expire_on_commit=False)


class Base(AsyncAttrs, DeclarativeBase):
    pass


async def get_db():
    async with async_session() as session:
        yield session

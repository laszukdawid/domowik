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

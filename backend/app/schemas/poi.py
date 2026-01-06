from pydantic import BaseModel


class POIResponse(BaseModel):
    id: int
    osm_id: int
    type: str
    name: str | None
    geometry: dict  # GeoJSON

    class Config:
        from_attributes = True

import { useEffect, useState } from 'react';
import { CircleMarker, Polygon, Tooltip, useMap } from 'react-leaflet';
import type { POI } from '../types';
import { usePOICache } from '../hooks/usePOICache';

const SHOW_POI_ZOOM_THRESHOLD = 14;

// POI type colors
const POI_COLORS: Record<string, string> = {
  coffee_shop: '#8B4513',  // Brown
  dog_park: '#F97316',     // Orange
  park: '#22C55E',         // Green
  garden: '#16A34A',       // Darker green
  playground: '#A855F7',   // Purple
};

interface POILayerProps {
  poiIds: number[];
}

function ZoomAwarePOILayer({ poiIds }: POILayerProps) {
  const map = useMap();
  const { fetchPOIs } = usePOICache();
  const [pois, setPois] = useState<POI[]>([]);
  const [zoom, setZoom] = useState(map.getZoom());

  // Track zoom changes
  useEffect(() => {
    const onZoomEnd = () => setZoom(map.getZoom());
    map.on('zoomend', onZoomEnd);
    return () => {
      map.off('zoomend', onZoomEnd);
    };
  }, [map]);

  // Fetch POIs when IDs change and zoom is sufficient
  useEffect(() => {
    if (zoom < SHOW_POI_ZOOM_THRESHOLD || poiIds.length === 0) {
      setPois([]);
      return;
    }

    fetchPOIs(poiIds).then(setPois);
  }, [poiIds, zoom, fetchPOIs]);

  if (zoom < SHOW_POI_ZOOM_THRESHOLD) {
    return null;
  }

  return (
    <>
      {pois.map((poi) => {
        const color = POI_COLORS[poi.type] || '#6B7280';

        if (poi.geometry.type === 'Point') {
          const [lng, lat] = poi.geometry.coordinates;
          return (
            <CircleMarker
              key={poi.id}
              center={[lat, lng]}
              radius={6}
              pathOptions={{
                fillColor: color,
                fillOpacity: 0.8,
                color: '#FFFFFF',
                weight: 2,
              }}
            >
              <Tooltip>
                <div className="text-sm">
                  <div className="font-semibold">{poi.name || poi.type}</div>
                  <div className="text-gray-500 capitalize">{poi.type.replace('_', ' ')}</div>
                </div>
              </Tooltip>
            </CircleMarker>
          );
        }

        if (poi.geometry.type === 'Polygon') {
          // Convert GeoJSON [lng, lat] to Leaflet [lat, lng]
          const positions = poi.geometry.coordinates[0].map(
            ([lng, lat]) => [lat, lng] as [number, number]
          );
          return (
            <Polygon
              key={poi.id}
              positions={positions}
              pathOptions={{
                color: color,
                weight: 2,
                fillColor: color,
                fillOpacity: 0.15,
              }}
            >
              <Tooltip>
                <div className="text-sm">
                  <div className="font-semibold">{poi.name || poi.type}</div>
                  <div className="text-gray-500 capitalize">{poi.type.replace('_', ' ')}</div>
                </div>
              </Tooltip>
            </Polygon>
          );
        }

        return null;
      })}
    </>
  );
}

export function POILayer({ poiIds }: POILayerProps) {
  // Only render if we have POI IDs
  if (!poiIds || poiIds.length === 0) {
    return null;
  }

  return <ZoomAwarePOILayer poiIds={poiIds} />;
}

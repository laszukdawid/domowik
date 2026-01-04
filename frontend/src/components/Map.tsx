import { useEffect, useRef } from 'react';
import { MapContainer, TileLayer, Marker, Popup, useMap, useMapEvents } from 'react-leaflet';
import L from 'leaflet';
import type { Listing, BBox } from '../types';

// Fix default marker icons
delete (L.Icon.Default.prototype as { _getIconUrl?: () => string })._getIconUrl;
L.Icon.Default.mergeOptions({
  iconRetinaUrl: 'https://unpkg.com/leaflet@1.9.4/dist/images/marker-icon-2x.png',
  iconUrl: 'https://unpkg.com/leaflet@1.9.4/dist/images/marker-icon.png',
  shadowUrl: 'https://unpkg.com/leaflet@1.9.4/dist/images/marker-shadow.png',
});

function getMarkerColor(score: number | null | undefined): string {
  if (!score) return '#6B7280'; // gray
  if (score >= 80) return '#22C55E'; // green
  if (score >= 60) return '#84CC16'; // lime
  if (score >= 40) return '#EAB308'; // yellow
  if (score >= 20) return '#F97316'; // orange
  return '#EF4444'; // red
}

function createMarkerIcon(listing: Listing, isSelected: boolean): L.DivIcon {
  const color = getMarkerColor(listing.amenity_score?.amenity_score);
  const size = isSelected ? 16 : 12;
  const border = listing.is_favorite ? '3px solid #FFD700' : '2px solid white';

  return L.divIcon({
    className: 'custom-marker',
    html: `<div style="
      width: ${size}px;
      height: ${size}px;
      background: ${color};
      border-radius: 50%;
      border: ${border};
      box-shadow: 0 2px 4px rgba(0,0,0,0.3);
    "></div>`,
    iconSize: [size, size],
    iconAnchor: [size / 2, size / 2],
  });
}

interface MapBoundsTrackerProps {
  onBoundsChange: (bbox: BBox, zoom: number) => void;
  debounceMs?: number;
}

function MapBoundsTracker({ onBoundsChange, debounceMs = 300 }: MapBoundsTrackerProps) {
  const map = useMap();
  const timeoutRef = useRef<number | null>(null);

  const emitBounds = () => {
    const bounds = map.getBounds();
    const zoom = map.getZoom();

    onBoundsChange({
      minLng: bounds.getWest(),
      minLat: bounds.getSouth(),
      maxLng: bounds.getEast(),
      maxLat: bounds.getNorth(),
    }, zoom);
  };

  useMapEvents({
    moveend: () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
      timeoutRef.current = window.setTimeout(emitBounds, debounceMs);
    },
    zoomend: () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
      timeoutRef.current = window.setTimeout(emitBounds, debounceMs);
    },
  });

  // Emit initial bounds on mount
  useEffect(() => {
    emitBounds();
    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
    };
  }, []);

  return null;
}

interface MapProps {
  listings: Listing[];
  selectedId?: number;
  onSelect: (listing: Listing) => void;
  onBoundsChange?: (bbox: BBox, zoom: number) => void;
}

function MapUpdater({ listings }: { listings: Listing[] }) {
  const map = useMap();

  // Center on Vancouver area if no listings
  if (listings.length === 0) {
    map.setView([49.2827, -123.1207], 11);
  }

  return null;
}

export default function Map({ listings, selectedId, onSelect, onBoundsChange }: MapProps) {
  const validListings = listings.filter((l) => l.latitude && l.longitude);

  return (
    <MapContainer
      center={[49.2827, -123.1207]}
      zoom={11}
      className="h-full w-full"
    >
      <TileLayer
        attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>'
        url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
      />
      <MapUpdater listings={validListings} />
      {onBoundsChange && <MapBoundsTracker onBoundsChange={onBoundsChange} />}

      {validListings.map((listing) => (
        <Marker
          key={listing.id}
          position={[listing.latitude!, listing.longitude!]}
          icon={createMarkerIcon(listing, listing.id === selectedId)}
          eventHandlers={{
            click: () => onSelect(listing),
          }}
        >
          <Popup>
            <div className="text-sm">
              <div className="font-semibold">{listing.address}</div>
              <div className="text-gray-600">
                ${listing.price.toLocaleString()}
              </div>
              {listing.amenity_score && (
                <div className="text-xs text-gray-500">
                  Score: {listing.amenity_score.amenity_score}
                </div>
              )}
            </div>
          </Popup>
        </Marker>
      ))}
    </MapContainer>
  );
}

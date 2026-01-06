import { useEffect, useRef, useState } from 'react';
import { MapContainer, TileLayer, Marker, Popup, useMap, useMapEvents, CircleMarker } from 'react-leaflet';
import L from 'leaflet';
import type { Listing, BBox, Cluster, ClusterOutlier } from '../types';

// Icon cache to prevent recreating icons on every render
const iconCache: Record<string, L.DivIcon> = {};

function getIconCacheKey(
  color: string,
  size: number,
  isFavorite: boolean
): string {
  return `${color}-${size}-${isFavorite}`;
}

function getCachedIcon(
  color: string,
  size: number,
  isFavorite: boolean
): L.DivIcon {
  const key = getIconCacheKey(color, size, isFavorite);
  let icon = iconCache[key];
  if (!icon) {
    const border = isFavorite ? '3px solid #FFD700' : '2px solid white';
    icon = L.divIcon({
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
    iconCache[key] = icon;
  }
  return icon;
}

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

function createOutlierIcon(outlier: ClusterOutlier): L.DivIcon {
  const color = getMarkerColor(outlier.amenity_score);
  return getCachedIcon(color, 12, outlier.is_favorite ?? false);
}

function createMarkerIcon(listing: Listing, isSelected: boolean): L.DivIcon {
  const color = getMarkerColor(listing.amenity_score?.amenity_score);
  const size = isSelected ? 16 : 12;
  return getCachedIcon(color, size, listing.is_favorite ?? false);
}

interface MapBoundsTrackerProps {
  onBoundsChange: (bbox: BBox, zoom: number) => void;
  debounceMs?: number;
}

// Round to 4 decimal places (~11m precision) to reduce unique bbox strings
function roundCoord(coord: number): number {
  return Math.round(coord * 10000) / 10000;
}

function MapBoundsTracker({ onBoundsChange, debounceMs = 500 }: MapBoundsTrackerProps) {
  const map = useMap();
  const timeoutRef = useRef<number | null>(null);
  const isUserInteracting = useRef(false);
  const onBoundsChangeRef = useRef(onBoundsChange);
  const lastEmittedRef = useRef<string | null>(null);

  // Keep callback ref updated without causing re-renders
  useEffect(() => {
    onBoundsChangeRef.current = onBoundsChange;
  }, [onBoundsChange]);

  const emitBounds = () => {
    const bounds = map.getBounds();
    const zoom = map.getZoom();

    const bbox = {
      minLng: roundCoord(bounds.getWest()),
      minLat: roundCoord(bounds.getSouth()),
      maxLng: roundCoord(bounds.getEast()),
      maxLat: roundCoord(bounds.getNorth()),
    };

    // Skip if bounds haven't meaningfully changed
    const boundsKey = `${bbox.minLng},${bbox.minLat},${bbox.maxLng},${bbox.maxLat},${zoom}`;
    if (boundsKey === lastEmittedRef.current) {
      return;
    }
    lastEmittedRef.current = boundsKey;

    onBoundsChangeRef.current(bbox, zoom);
  };

  useMapEvents({
    // Track user interaction start
    dragstart: () => {
      isUserInteracting.current = true;
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
        timeoutRef.current = null;
      }
    },
    zoomstart: () => {
      isUserInteracting.current = true;
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
        timeoutRef.current = null;
      }
    },
    // Only emit bounds after user stops interacting
    moveend: () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
      timeoutRef.current = window.setTimeout(() => {
        isUserInteracting.current = false;
        emitBounds();
      }, debounceMs);
    },
    zoomend: () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
      timeoutRef.current = window.setTimeout(() => {
        isUserInteracting.current = false;
        emitBounds();
      }, debounceMs);
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
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return null;
}

interface ClusterMarkerProps {
  center: { lat: number; lng: number };
  count: number;
  label: string;
  onClick: () => void;
}

function ClusterMarker({ center, count, label, onClick }: ClusterMarkerProps) {
  // Size based on count
  const size = Math.min(60, Math.max(30, 20 + Math.log10(count) * 15));

  return (
    <CircleMarker
      center={[center.lat, center.lng]}
      radius={size / 2}
      pathOptions={{
        fillColor: '#3B82F6',
        fillOpacity: 0.8,
        color: '#1D4ED8',
        weight: 2,
      }}
      eventHandlers={{ click: onClick }}
    >
      <Popup>
        <div className="text-center">
          <div className="font-semibold">{label}</div>
          <div className="text-sm text-gray-600">{count} listings</div>
        </div>
      </Popup>
    </CircleMarker>
  );
}

interface MapProps {
  listings: Listing[];
  clusters?: Cluster[];
  outliers?: ClusterOutlier[];
  selectedId?: number;
  onSelect: (listing: Listing | ClusterOutlier) => void;
  onBoundsChange?: (bbox: BBox, zoom: number) => void;
  onClusterClick?: (cluster: Cluster) => void;
}

// Removed MapUpdater - it was calling setView on every render when listings=0,
// causing the map to "bounce back" to Vancouver when panning/zooming.

/**
 * Component that defers marker rendering while user is interacting with map.
 * This prevents React re-renders from interfering with Leaflet's drag/zoom.
 */
interface DeferredMarkersProps {
  listings: Listing[];
  clusters: Cluster[];
  outliers: ClusterOutlier[];
  selectedId?: number;
  onSelect: (listing: Listing | ClusterOutlier) => void;
  onClusterClick?: (cluster: Cluster) => void;
}

interface DisplayedState {
  listings: Listing[];
  clusters: Cluster[];
  outliers: ClusterOutlier[];
  selectedId?: number;
}

function DeferredMarkers({
  listings,
  clusters,
  outliers,
  selectedId,
  onSelect,
  onClusterClick,
}: DeferredMarkersProps) {
  const isInteracting = useRef(false);
  const pendingUpdate = useRef(false);

  // Store the latest props in refs so we can use them after interaction ends
  const propsRef = useRef<DisplayedState>({ listings, clusters, outliers, selectedId });
  propsRef.current = { listings, clusters, outliers, selectedId };

  // Track displayed state separately to defer updates during interaction
  const [displayed, setDisplayed] = useState<DisplayedState>({ listings, clusters, outliers, selectedId });

  useMapEvents({
    dragstart: () => {
      isInteracting.current = true;
    },
    zoomstart: () => {
      isInteracting.current = true;
    },
    moveend: () => {
      // Small delay to let any in-flight animations complete
      setTimeout(() => {
        if (isInteracting.current) {
          isInteracting.current = false;
          if (pendingUpdate.current) {
            pendingUpdate.current = false;
            setDisplayed(propsRef.current);
          }
        }
      }, 50);
    },
    zoomend: () => {
      setTimeout(() => {
        if (isInteracting.current) {
          isInteracting.current = false;
          if (pendingUpdate.current) {
            pendingUpdate.current = false;
            setDisplayed(propsRef.current);
          }
        }
      }, 50);
    },
  });

  // Update displayed markers when props change, but defer if interacting
  useEffect(() => {
    if (isInteracting.current) {
      pendingUpdate.current = true;
    } else {
      setDisplayed({ listings, clusters, outliers, selectedId });
    }
  }, [listings, clusters, outliers, selectedId]);

  const validListings = displayed.listings.filter((l) => l.latitude && l.longitude);

  return (
    <>
      {/* Cluster markers */}
      {displayed.clusters.map((cluster) => (
        <ClusterMarker
          key={cluster.id}
          center={cluster.center}
          count={cluster.count}
          label={cluster.label}
          onClick={() => onClusterClick?.(cluster)}
        />
      ))}

      {/* Outlier markers */}
      {displayed.outliers.map((outlier) => (
        <Marker
          key={`outlier-${outlier.id}`}
          position={[outlier.lat, outlier.lng]}
          icon={createOutlierIcon(outlier)}
          eventHandlers={{
            click: () => onSelect(outlier as unknown as Listing),
          }}
        >
          <Popup>
            <div className="text-sm">
              <div className="font-semibold">{outlier.address}</div>
              <div className="text-gray-600">${outlier.price.toLocaleString()}</div>
            </div>
          </Popup>
        </Marker>
      ))}

      {/* Individual listing markers */}
      {validListings.map((listing) => (
        <Marker
          key={listing.id}
          position={[listing.latitude!, listing.longitude!]}
          icon={createMarkerIcon(listing, listing.id === displayed.selectedId)}
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
    </>
  );
}

export default function Map({
  listings,
  clusters = [],
  outliers = [],
  selectedId,
  onSelect,
  onBoundsChange,
  onClusterClick,
}: MapProps) {
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
      {onBoundsChange && <MapBoundsTracker onBoundsChange={onBoundsChange} />}
      <DeferredMarkers
        listings={listings}
        clusters={clusters}
        outliers={outliers}
        selectedId={selectedId}
        onSelect={onSelect}
        onClusterClick={onClusterClick}
      />
    </MapContainer>
  );
}

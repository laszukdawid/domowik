import { useEffect, useRef, useState } from 'react';
import { MapContainer, TileLayer, Marker, Popup, useMap, useMapEvents, CircleMarker } from 'react-leaflet';
import L from 'leaflet';
import type { Listing, BBox, Cluster, ClusterOutlier } from '../types';
import { getMarkerColor, getScoreBadgeSolidClasses } from '../utils/scoreColors';

// Icon cache to prevent recreating icons on every render
const iconCache: Record<string, L.DivIcon> = {};

function getIconCacheKey(
  color: string,
  size: number,
  isFavorite: boolean,
  isHighlighted: boolean
): string {
  return `${color}-${size}-${isFavorite}-${isHighlighted}`;
}

function getCachedIcon(
  color: string,
  size: number,
  isFavorite: boolean,
  isHighlighted: boolean = false
): L.DivIcon {
  const key = getIconCacheKey(color, size, isFavorite, isHighlighted);
  let icon = iconCache[key];
  if (!icon) {
    let border: string;
    let boxShadow: string;
    if (isHighlighted) {
      border = '3px solid #3B82F6'; // Blue highlight border
      boxShadow = '0 0 0 4px rgba(59, 130, 246, 0.4), 0 2px 4px rgba(0,0,0,0.3)';
    } else if (isFavorite) {
      border = '3px solid #FFD700';
      boxShadow = '0 2px 4px rgba(0,0,0,0.3)';
    } else {
      border = '2px solid white';
      boxShadow = '0 2px 4px rgba(0,0,0,0.3)';
    }
    icon = L.divIcon({
      className: 'custom-marker',
      html: `<div style="
        width: ${size}px;
        height: ${size}px;
        background: ${color};
        border-radius: 50%;
        border: ${border};
        box-shadow: ${boxShadow};
        transition: all 0.15s ease-out;
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


function createOutlierIcon(outlier: ClusterOutlier, isHovered: boolean): L.DivIcon {
  const color = getMarkerColor(outlier.amenity_score);
  const size = isHovered ? 20 : 12;
  return getCachedIcon(color, size, outlier.is_favorite ?? false, isHovered);
}

function createMarkerIcon(listing: Listing, isSelected: boolean, isHovered: boolean): L.DivIcon {
  const color = getMarkerColor(listing.amenity_score?.amenity_score);
  const size = isHovered ? 20 : isSelected ? 16 : 12;
  return getCachedIcon(color, size, listing.is_favorite ?? false, isHovered);
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
  isHovered: boolean;
  onClick: () => void;
}

function ClusterMarker({ center, count, label, isHovered, onClick }: ClusterMarkerProps) {
  // Size based on count, larger when hovered
  const baseSize = Math.min(60, Math.max(30, 20 + Math.log10(count) * 15));
  const size = isHovered ? baseSize * 1.4 : baseSize;

  return (
    <CircleMarker
      center={[center.lat, center.lng]}
      radius={size / 2}
      pathOptions={{
        fillColor: isHovered ? '#2563EB' : '#3B82F6',
        fillOpacity: isHovered ? 1 : 0.8,
        color: isHovered ? '#1E40AF' : '#1D4ED8',
        weight: isHovered ? 4 : 2,
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
  hoveredId?: number | null;
  hoveredClusterId?: string | null;
  focusCluster?: Cluster | null;
  onSelect: (listing: Listing | ClusterOutlier) => void;
  onBoundsChange?: (bbox: BBox, zoom: number) => void;
  onClusterClick?: (cluster: Cluster) => void;
}

/**
 * Component that pans/zooms the map to fit a cluster's bounds
 */
function ClusterFocus({ cluster }: { cluster: Cluster | null }) {
  const map = useMap();
  const lastFocusedRef = useRef<string | null>(null);

  useEffect(() => {
    if (!cluster) {
      lastFocusedRef.current = null;
      return;
    }

    // Avoid re-focusing the same cluster
    if (lastFocusedRef.current === cluster.id) {
      return;
    }
    lastFocusedRef.current = cluster.id;

    const { bounds } = cluster;
    const leafletBounds = L.latLngBounds(
      [bounds.south, bounds.west],
      [bounds.north, bounds.east]
    );

    // Fit the map to the cluster bounds with some padding
    map.fitBounds(leafletBounds, {
      padding: [50, 50],
      maxZoom: 16, // Don't zoom in too far
      animate: true,
      duration: 0.5,
    });
  }, [cluster, map]);

  return null;
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
  hoveredId?: number | null;
  hoveredClusterId?: string | null;
  onSelect: (listing: Listing | ClusterOutlier) => void;
  onClusterClick?: (cluster: Cluster) => void;
}

interface DisplayedState {
  listings: Listing[];
  clusters: Cluster[];
  outliers: ClusterOutlier[];
  selectedId?: number;
  hoveredId?: number | null;
  hoveredClusterId?: string | null;
}

function DeferredMarkers({
  listings,
  clusters,
  outliers,
  selectedId,
  hoveredId,
  hoveredClusterId,
  onSelect,
  onClusterClick,
}: DeferredMarkersProps) {
  const isInteracting = useRef(false);
  const pendingUpdate = useRef(false);

  // Store the latest props in refs so we can use them after interaction ends
  const propsRef = useRef<DisplayedState>({ listings, clusters, outliers, selectedId, hoveredId, hoveredClusterId });
  propsRef.current = { listings, clusters, outliers, selectedId, hoveredId, hoveredClusterId };

  // Track displayed state separately to defer updates during interaction
  const [displayed, setDisplayed] = useState<DisplayedState>({ listings, clusters, outliers, selectedId, hoveredId, hoveredClusterId });

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
  // Exception: hoveredId changes should update immediately for responsiveness
  useEffect(() => {
    if (isInteracting.current) {
      pendingUpdate.current = true;
    } else {
      setDisplayed({ listings, clusters, outliers, selectedId, hoveredId, hoveredClusterId });
    }
  }, [listings, clusters, outliers, selectedId, hoveredId, hoveredClusterId]);

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
          isHovered={cluster.id === displayed.hoveredClusterId}
          onClick={() => onClusterClick?.(cluster)}
        />
      ))}

      {/* Outlier markers */}
      {displayed.outliers.map((outlier) => (
        <Marker
          key={`outlier-${outlier.id}`}
          position={[outlier.lat, outlier.lng]}
          icon={createOutlierIcon(outlier, outlier.id === displayed.hoveredId)}
          eventHandlers={{
            click: () => onSelect(outlier as unknown as Listing),
          }}
        >
          <Popup>
            <div className="text-sm min-w-[180px]">
              <div className="flex items-start justify-between gap-2">
                <div>
                  <div className="font-semibold">{outlier.address}</div>
                  <div className="text-green-600 font-medium">${outlier.price.toLocaleString()}</div>
                </div>
                {outlier.url && (
                  <a
                    href={outlier.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-blue-600 hover:text-blue-800 flex-shrink-0"
                    title="View on Realtor"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" viewBox="0 0 20 20" fill="currentColor">
                      <path d="M11 3a1 1 0 100 2h2.586l-6.293 6.293a1 1 0 101.414 1.414L15 6.414V9a1 1 0 102 0V4a1 1 0 00-1-1h-5z" />
                      <path d="M5 5a2 2 0 00-2 2v8a2 2 0 002 2h8a2 2 0 002-2v-3a1 1 0 10-2 0v3H5V7h3a1 1 0 000-2H5z" />
                    </svg>
                  </a>
                )}
              </div>
              {outlier.amenity_score != null && (
                <div className="mt-1 flex items-center gap-1">
                  <span className={`inline-block px-1.5 py-0.5 rounded text-xs font-medium text-white ${getScoreBadgeSolidClasses(outlier.amenity_score)}`}>
                    {outlier.amenity_score}
                  </span>
                  <span className="text-xs text-gray-500">score</span>
                </div>
              )}
            </div>
          </Popup>
        </Marker>
      ))}

      {/* Individual listing markers */}
      {validListings.map((listing) => (
        <Marker
          key={listing.id}
          position={[listing.latitude!, listing.longitude!]}
          icon={createMarkerIcon(listing, listing.id === displayed.selectedId, listing.id === displayed.hoveredId)}
          eventHandlers={{
            click: () => onSelect(listing),
          }}
        >
          <Popup>
            <div className="text-sm min-w-[180px]">
              <div className="flex items-start justify-between gap-2">
                <div>
                  <div className="font-semibold">{listing.address}</div>
                  <div className="text-green-600 font-medium">
                    ${listing.price.toLocaleString()}
                  </div>
                </div>
                {listing.url && (
                  <a
                    href={listing.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-blue-600 hover:text-blue-800 flex-shrink-0"
                    title="View on Realtor"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" viewBox="0 0 20 20" fill="currentColor">
                      <path d="M11 3a1 1 0 100 2h2.586l-6.293 6.293a1 1 0 101.414 1.414L15 6.414V9a1 1 0 102 0V4a1 1 0 00-1-1h-5z" />
                      <path d="M5 5a2 2 0 00-2 2v8a2 2 0 002 2h8a2 2 0 002-2v-3a1 1 0 10-2 0v3H5V7h3a1 1 0 000-2H5z" />
                    </svg>
                  </a>
                )}
              </div>
              {listing.amenity_score?.amenity_score != null && (
                <div className="mt-1 flex items-center gap-1">
                  <span className={`inline-block px-1.5 py-0.5 rounded text-xs font-medium text-white ${getScoreBadgeSolidClasses(listing.amenity_score.amenity_score)}`}>
                    {listing.amenity_score.amenity_score}
                  </span>
                  <span className="text-xs text-gray-500">score</span>
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
  hoveredId,
  hoveredClusterId,
  focusCluster,
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
      <ClusterFocus cluster={focusCluster ?? null} />
      <DeferredMarkers
        listings={listings}
        clusters={clusters}
        outliers={outliers}
        selectedId={selectedId}
        hoveredId={hoveredId}
        hoveredClusterId={hoveredClusterId}
        onSelect={onSelect}
        onClusterClick={onClusterClick}
      />
    </MapContainer>
  );
}

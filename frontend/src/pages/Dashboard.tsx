import { useState, useCallback, useEffect } from 'react';
import { useAuth } from '../hooks/useAuth';
import { useListings, useListing } from '../hooks/useListings';
import { useClusters } from '../hooks/useClusters';
import { usePersistedFilters } from '../hooks/usePersistedFilters';
import type { Listing, BBox, Cluster, ClusterOutlier } from '../types';
import Map from '../components/Map';
import ListingSidebar from '../components/ListingSidebar';
import ListingDetail from '../components/ListingDetail';
import FilterBar from '../components/FilterBar';

export default function Dashboard() {
  const { user, logout } = useAuth();
  const [filterGroups, setFilterGroups] = usePersistedFilters();
  const [selectedListing, setSelectedListing] = useState<Listing | null>(null);
  const [selectedOutlierId, setSelectedOutlierId] = useState<number | null>(null);
  const [mapBounds, setMapBounds] = useState<{ bbox: BBox; zoom: number } | null>(null);
  const [expandedCluster, setExpandedCluster] = useState<Cluster | null>(null);
  const [hoveredListingId, setHoveredListingId] = useState<number | null>(null);
  const [hoveredClusterId, setHoveredClusterId] = useState<string | null>(null);
  const [isDrawingPolygon, setIsDrawingPolygon] = useState(false);
  const [currentPolygon, setCurrentPolygon] = useState<number[][]>([]);

  // Fetch individual listing when an outlier is clicked
  const { data: fetchedListing } = useListing(selectedOutlierId ?? 0);

  // Update selectedListing when the fetched listing arrives
  useEffect(() => {
    if (fetchedListing && selectedOutlierId) {
      setSelectedListing(fetchedListing);
      setSelectedOutlierId(null);
    }
  }, [fetchedListing, selectedOutlierId]);

  // Only fetch clusters - this is lightweight
  const {
    data: clusterData,
    isLoading: clustersLoading
  } = useClusters({
    bbox: mapBounds?.bbox ?? null,
    zoom: mapBounds?.zoom ?? 11,
    filterGroups,
    enabled: !!mapBounds,
  });

  // Only fetch full listings when zoomed in far enough OR when a cluster is expanded
  const shouldFetchListings = (mapBounds && mapBounds.zoom >= 15) || expandedCluster !== null;

  // Add a small buffer to cluster bounds to ensure all edge points are captured
  // ST_Within may exclude points exactly on the boundary due to floating-point precision
  const CLUSTER_BBOX_BUFFER = 0.0001; // ~11 meters at equator

  const listingsBbox = expandedCluster
    ? `${expandedCluster.bounds.west - CLUSTER_BBOX_BUFFER},${expandedCluster.bounds.south - CLUSTER_BBOX_BUFFER},${expandedCluster.bounds.east + CLUSTER_BBOX_BUFFER},${expandedCluster.bounds.north + CLUSTER_BBOX_BUFFER}`
    : mapBounds
      ? `${mapBounds.bbox.minLng},${mapBounds.bbox.minLat},${mapBounds.bbox.maxLng},${mapBounds.bbox.maxLat}`
      : undefined;

  const { data: listings = [], isStreaming } = useListings(
    filterGroups,
    shouldFetchListings ? listingsBbox : undefined
  );

  const clusters = clusterData?.clusters ?? [];
  const outliers = clusterData?.outliers ?? [];
  const totalCount = clusters.reduce((sum, c) => sum + c.count, 0) + outliers.length;

  const handleBoundsChange = useCallback((bbox: BBox, zoom: number) => {
    setMapBounds({ bbox, zoom });
    // Clear expanded cluster when viewport changes
    setExpandedCluster(null);
  }, []);

  const handleSelect = (item: Listing | ClusterOutlier) => {
    // Check if it's an outlier (has lat/lng instead of latitude/longitude, and amenity_score is a number not object)
    const isOutlier = 'lat' in item && 'lng' in item;

    if (isOutlier) {
      // Try to find full listing data in already-loaded listings
      const fullListing = listings.find(l => l.id === item.id);
      if (fullListing) {
        setSelectedListing(fullListing);
      } else {
        // Fetch the full listing on-demand
        setSelectedOutlierId(item.id);
      }
    } else {
      setSelectedListing(item as Listing);
    }
  };

  const handleClusterClick = (cluster: Cluster) => {
    // Expand cluster - this triggers fetching listings for that cluster's bounds
    setExpandedCluster(cluster);
  };

  const handleClusterCollapse = () => {
    setExpandedCluster(null);
  };

  const handleStartDrawing = () => {
    setIsDrawingPolygon(true);
    setCurrentPolygon([]);
  };

  const handleMapClick = useCallback((latlng: { lat: number; lng: number }) => {
    if (isDrawingPolygon) {
      setCurrentPolygon(prev => [...prev, [latlng.lng, latlng.lat]]);
    }
  }, [isDrawingPolygon]);

  const handleFinishPolygon = () => {
    if (currentPolygon.length >= 3) {
      const existingPolygons = filterGroups.polygons || [];
      setFilterGroups({
        ...filterGroups,
        polygons: [...existingPolygons, currentPolygon]
      });
    }
    setIsDrawingPolygon(false);
    setCurrentPolygon([]);
  };

  const handleCancelDrawing = () => {
    setIsDrawingPolygon(false);
    setCurrentPolygon([]);
  };

  // Filter listings for expanded cluster
  const expandedListings = expandedCluster
    ? listings.filter(l => expandedCluster.listing_ids.includes(l.id))
    : [];

  return (
    <div className="h-screen flex flex-col">
      {/* Header */}
      <header className="bg-white shadow px-4 py-3 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h1 className="text-xl font-bold">Domowik</h1>
          {(clustersLoading || isStreaming) && (
            <div className="flex items-center gap-2 text-sm text-blue-600">
              <div className="animate-spin h-4 w-4 border-2 border-blue-600 border-t-transparent rounded-full"></div>
              <span>Loading...</span>
            </div>
          )}
        </div>
        <div className="flex items-center gap-4">
          {isDrawingPolygon ? (
            <>
              <div className="flex items-center gap-2">
                <span className="text-sm text-blue-600 font-medium">
                  Drawing polygon ({currentPolygon.length} points)
                </span>
                <button
                  onClick={handleFinishPolygon}
                  disabled={currentPolygon.length < 3}
                  className="px-3 py-1 text-sm bg-green-500 text-white rounded hover:bg-green-600 disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  Finish
                </button>
                <button
                  onClick={handleCancelDrawing}
                  className="px-3 py-1 text-sm bg-red-500 text-white rounded hover:bg-red-600"
                >
                  Cancel
                </button>
              </div>
            </>
          ) : (
            <>
              <button
                onClick={handleStartDrawing}
                className="px-3 py-1 bg-blue-500 text-white text-sm rounded hover:bg-blue-600"
              >
                Draw Polygon
              </button>
              <FilterBar filterGroups={filterGroups} onChange={setFilterGroups} />
            </>
          )}
          <span className="text-sm text-gray-600">{user?.name}</span>
          <button
            onClick={logout}
            className="text-sm text-gray-500 hover:text-gray-700"
          >
            Logout
          </button>
        </div>
      </header>

      {/* Main content */}
      <div className="flex-1 flex overflow-hidden">
        {/* Map */}
        <div className="flex-1 relative">
          <Map
            listings={mapBounds && mapBounds.zoom >= 15 ? listings : []}
            clusters={mapBounds && mapBounds.zoom < 15 ? clusters : []}
            outliers={mapBounds && mapBounds.zoom < 15 ? outliers : []}
            selectedId={selectedListing?.id}
            hoveredId={hoveredListingId}
            hoveredClusterId={hoveredClusterId}
            focusCluster={expandedCluster}
            selectedPoiIds={selectedListing?.poi_ids}
            onSelect={handleSelect}
            onBoundsChange={handleBoundsChange}
            onClusterClick={handleClusterClick}
            isDrawing={isDrawingPolygon}
            currentPolygon={currentPolygon}
            polygons={filterGroups.polygons}
            onMapClick={handleMapClick}
          />
        </div>

        {/* Sidebar */}
        <div className="w-80 bg-white border-l overflow-hidden">
          {selectedListing ? (
            <ListingDetail
              listing={selectedListing}
              onClose={() => setSelectedListing(null)}
            />
          ) : (
            <ListingSidebar
              clusters={clusters}
              outliers={outliers}
              listings={expandedListings}
              isLoading={clustersLoading || (expandedCluster !== null && isStreaming)}
              totalCount={totalCount}
              onSelect={handleSelect}
              onClusterClick={handleClusterClick}
              expandedCluster={expandedCluster}
              onBack={handleClusterCollapse}
              onHover={setHoveredListingId}
              onClusterHover={setHoveredClusterId}
            />
          )}
        </div>
      </div>
    </div>
  );
}

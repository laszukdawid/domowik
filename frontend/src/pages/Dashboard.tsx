import { useState, useCallback } from 'react';
import { useAuth } from '../hooks/useAuth';
import { useListings } from '../hooks/useListings';
import { useClusters } from '../hooks/useClusters';
import { usePersistedFilters } from '../hooks/usePersistedFilters';
import type { Listing, BBox, Cluster, ClusterOutlier } from '../types';
import Map from '../components/Map';
import ListingSidebar from '../components/ListingSidebar';
import ListingDetail from '../components/ListingDetail';
import FilterBar from '../components/FilterBar';

export default function Dashboard() {
  const { user, logout } = useAuth();
  const [filters, setFilters] = usePersistedFilters();
  const [selectedListing, setSelectedListing] = useState<Listing | null>(null);
  const [mapBounds, setMapBounds] = useState<{ bbox: BBox; zoom: number } | null>(null);
  const [expandedCluster, setExpandedCluster] = useState<Cluster | null>(null);

  // Only fetch clusters - this is lightweight
  const {
    data: clusterData,
    isLoading: clustersLoading
  } = useClusters({
    bbox: mapBounds?.bbox ?? null,
    zoom: mapBounds?.zoom ?? 11,
    filters,
    enabled: !!mapBounds,
  });

  // Only fetch full listings when zoomed in far enough OR when a cluster is expanded
  const shouldFetchListings = (mapBounds && mapBounds.zoom >= 15) || expandedCluster !== null;

  const listingsBbox = expandedCluster
    ? `${expandedCluster.bounds.west},${expandedCluster.bounds.south},${expandedCluster.bounds.east},${expandedCluster.bounds.north}`
    : mapBounds
      ? `${mapBounds.bbox.minLng},${mapBounds.bbox.minLat},${mapBounds.bbox.maxLng},${mapBounds.bbox.maxLat}`
      : undefined;

  const { data: listings = [], isStreaming } = useListings({
    ...filters,
    bbox: shouldFetchListings ? listingsBbox : undefined,
  });

  const clusters = clusterData?.clusters ?? [];
  const outliers = clusterData?.outliers ?? [];
  const totalCount = clusters.reduce((sum, c) => sum + c.count, 0) + outliers.length;

  const handleBoundsChange = useCallback((bbox: BBox, zoom: number) => {
    setMapBounds({ bbox, zoom });
    // Clear expanded cluster when viewport changes
    setExpandedCluster(null);
  }, []);

  const handleSelect = (item: Listing | ClusterOutlier) => {
    // If it's an outlier, try to find full listing data
    if ('address' in item && !('amenity_score' in item && typeof item.amenity_score === 'object')) {
      const fullListing = listings.find(l => l.id === item.id);
      if (fullListing) {
        setSelectedListing(fullListing);
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

  // Filter listings for expanded cluster
  const expandedListings = expandedCluster
    ? listings.filter(l => expandedCluster.listing_ids.includes(l.id))
    : [];

  return (
    <div className="h-screen flex flex-col">
      {/* Header */}
      <header className="bg-white shadow px-4 py-3 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h1 className="text-xl font-bold">HomeHero</h1>
          {(clustersLoading || isStreaming) && (
            <div className="flex items-center gap-2 text-sm text-blue-600">
              <div className="animate-spin h-4 w-4 border-2 border-blue-600 border-t-transparent rounded-full"></div>
              <span>Loading...</span>
            </div>
          )}
        </div>
        <div className="flex items-center gap-4">
          <FilterBar filters={filters} onChange={setFilters} />
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
            onSelect={handleSelect}
            onBoundsChange={handleBoundsChange}
            onClusterClick={handleClusterClick}
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
            />
          )}
        </div>
      </div>
    </div>
  );
}

import { useCallback } from 'react';
import { FixedSizeList as List } from 'react-window';
import type { Listing, Cluster, ClusterOutlier } from '../types';
import ListingCard from './ListingCard';
import ClusterCard from './ClusterCard';

interface ListingSidebarProps {
  clusters: Cluster[];
  outliers: ClusterOutlier[];
  listings: Listing[];  // Full listing data for expanded view
  isLoading: boolean;
  totalCount: number;
  onSelect: (listing: Listing | ClusterOutlier) => void;
  onClusterClick: (cluster: Cluster) => void;
  expandedCluster: Cluster | null;
  onBack: () => void;
}

export default function ListingSidebar({
  clusters,
  outliers,
  listings,
  isLoading,
  totalCount,
  onSelect,
  onClusterClick,
  expandedCluster,
  onBack,
}: ListingSidebarProps) {

  // Virtualized row renderer for expanded view
  const Row = useCallback(({ index, style }: { index: number; style: React.CSSProperties }) => {
    const listing = listings[index];
    if (!listing) return null;
    return (
      <div style={style} className="px-4 py-1">
        <ListingCard
          listing={listing}
          onClick={() => onSelect(listing)}
          compact
        />
      </div>
    );
  }, [listings, onSelect]);

  // Show expanded cluster view
  if (expandedCluster) {
    return (
      <div className="h-full flex flex-col">
        <div className="p-4 border-b bg-gray-50">
          <button
            onClick={onBack}
            className="flex items-center gap-2 text-sm text-gray-600 hover:text-gray-900 mb-2"
          >
            <span>←</span>
            <span>Back to areas</span>
          </button>
          <h2 className="font-semibold text-gray-900">
            {expandedCluster.label} ({expandedCluster.count})
          </h2>
          {isLoading && (
            <div className="text-sm text-blue-600 mt-1">Loading listings...</div>
          )}
        </div>

        <div className="flex-1">
          {listings.length > 0 ? (
            <List
              height={600}
              itemCount={listings.length}
              itemSize={80}
              width="100%"
            >
              {Row}
            </List>
          ) : !isLoading ? (
            <div className="p-4 text-gray-500">No listings found</div>
          ) : null}
        </div>
      </div>
    );
  }

  // Show aggregated view
  return (
    <div className="p-4">
      <div className="mb-4">
        <h2 className="text-sm font-semibold text-gray-500 uppercase">
          {isLoading ? 'Loading...' : `Viewport: ${totalCount} listings`}
        </h2>
      </div>

      {clusters.length > 0 && (
        <div className="space-y-2 mb-4">
          {clusters.map((cluster) => (
            <ClusterCard
              key={cluster.id}
              cluster={cluster}
              onClick={() => onClusterClick(cluster)}
            />
          ))}
        </div>
      )}

      {outliers.length > 0 && (
        <div>
          <h3 className="text-xs font-semibold text-gray-400 uppercase mb-2">
            Individual listings ({outliers.length})
          </h3>
          <div className="space-y-2">
            {outliers.slice(0, 10).map((outlier) => (
              <div
                key={outlier.id}
                onClick={() => onSelect(outlier)}
                className="p-2 rounded border border-gray-200 hover:border-gray-300 cursor-pointer bg-white hover:bg-gray-50"
              >
                <div className="text-sm font-medium truncate">{outlier.address}</div>
                <div className="text-sm text-gray-600">
                  ${outlier.price.toLocaleString()}
                  {outlier.bedrooms && ` · ${outlier.bedrooms} bd`}
                </div>
              </div>
            ))}
            {outliers.length > 10 && (
              <div className="text-sm text-gray-500 text-center">
                +{outliers.length - 10} more
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

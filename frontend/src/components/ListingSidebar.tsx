import { useCallback, useState, useMemo } from 'react';
import { FixedSizeList as List } from 'react-window';
import type { Listing, Cluster, ClusterOutlier } from '../types';
import ListingCard from './ListingCard';
import ClusterCard from './ClusterCard';
import CustomListingInput from './CustomListingInput';
import { getScoreBadgeClasses } from '../utils/scoreColors';

type SortField = 'score' | 'price';
type SortDirection = 'asc' | 'desc';

// Gauge icon for score sorting
function GaugeIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <path d="M12 22c5.523 0 10-4.477 10-10S17.523 2 12 2 2 6.477 2 12s4.477 10 10 10z" />
      <path d="M12 6v6l4 2" />
    </svg>
  );
}

// Dollar icon for price sorting
function DollarIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <line x1="12" y1="1" x2="12" y2="23" />
      <path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6" />
    </svg>
  );
}

// Arrow up icon
function ArrowUpIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <path d="M12 19V5M5 12l7-7 7 7" />
    </svg>
  );
}

// Arrow down icon
function ArrowDownIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <path d="M12 5v14M5 12l7 7 7-7" />
    </svg>
  );
}

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
  onHover: (id: number | null) => void;
  onClusterHover: (id: string | null) => void;
  // New props for custom lists
  selectedListId: number | null;
  onAddListing?: (input: string) => Promise<void>;
  onRemoveListing?: (listingId: number) => void;
  addListingLoading?: boolean;
  addListingError?: string | null;
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
  onHover,
  onClusterHover,
  selectedListId,
  onAddListing,
  onRemoveListing,
  addListingLoading,
  addListingError,
}: ListingSidebarProps) {
  const [sortField, setSortField] = useState<SortField>('score');
  const [sortDirection, setSortDirection] = useState<SortDirection>('desc');

  // Sort outliers based on current sort settings
  const sortedOutliers = useMemo(() => {
    return [...outliers].sort((a, b) => {
      let comparison = 0;
      if (sortField === 'score') {
        const scoreA = a.amenity_score ?? -1;
        const scoreB = b.amenity_score ?? -1;
        comparison = scoreA - scoreB;
      } else {
        comparison = a.price - b.price;
      }
      return sortDirection === 'desc' ? -comparison : comparison;
    });
  }, [outliers, sortField, sortDirection]);

  const handleSortClick = (field: SortField) => {
    if (sortField === field) {
      // Toggle direction if same field
      setSortDirection(prev => prev === 'desc' ? 'asc' : 'desc');
    } else {
      // Switch field, default to desc for score, asc for price
      setSortField(field);
      setSortDirection(field === 'score' ? 'desc' : 'asc');
    }
  };

  // Virtualized row renderer for expanded view
  const Row = useCallback(({ index, style }: { index: number; style: React.CSSProperties }) => {
    const listing = listings[index];
    if (!listing) return null;
    return (
      <div
        style={style}
        className="px-4 py-1"
        onMouseEnter={() => onHover(listing.id)}
        onMouseLeave={() => onHover(null)}
      >
        <ListingCard
          listing={listing}
          onClick={() => onSelect(listing)}
          compact
        />
      </div>
    );
  }, [listings, onSelect, onHover]);

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
    <div className="h-full overflow-y-auto p-4">
      <div className="mb-4">
        <h2 className="text-sm font-semibold text-gray-500 uppercase">
          {isLoading ? 'Loading...' : `Viewport: ${totalCount} listings`}
        </h2>
      </div>

      {/* Empty state for custom lists */}
      {selectedListId !== null && totalCount === 0 && !isLoading && (
        <div className="flex flex-col items-center justify-center py-12 px-4 text-center">
          <div className="w-16 h-16 mb-4 text-gray-300">
            <svg fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
            </svg>
          </div>
          <h3 className="text-lg font-medium text-gray-900 mb-2">No listings yet</h3>
          <p className="text-sm text-gray-500 mb-6">
            Add listings from realtor.ca to build your custom list
          </p>
          <div className="w-full max-w-sm">
            <CustomListingInput
              onSubmit={onAddListing!}
              isLoading={addListingLoading}
              error={addListingError}
            />
          </div>
        </div>
      )}

      {/* Add listing input for custom lists with items */}
      {selectedListId !== null && totalCount > 0 && onAddListing && (
        <div className="mb-4">
          <CustomListingInput
            onSubmit={onAddListing}
            isLoading={addListingLoading}
            error={addListingError}
          />
        </div>
      )}

      {clusters.length > 0 && (
        <div className="space-y-2 mb-4">
          {clusters.map((cluster) => (
            <ClusterCard
              key={cluster.id}
              cluster={cluster}
              onClick={() => onClusterClick(cluster)}
              onHover={onClusterHover}
            />
          ))}
        </div>
      )}

      {outliers.length > 0 && (
        <div>
          <div className="flex items-center justify-between mb-2">
            <h3 className="text-xs font-semibold text-gray-400 uppercase">
              Individual listings ({outliers.length})
            </h3>
            <div className="flex gap-1">
              <button
                onClick={() => handleSortClick('score')}
                className={`flex items-center gap-0.5 px-1.5 py-1 rounded text-xs ${
                  sortField === 'score'
                    ? 'bg-blue-100 text-blue-700'
                    : 'bg-gray-100 text-gray-600 hover:bg-gray-200'
                }`}
                title={`Sort by score ${sortField === 'score' ? (sortDirection === 'desc' ? '(highest first)' : '(lowest first)') : ''}`}
              >
                <GaugeIcon className="w-3.5 h-3.5" />
                {sortField === 'score' && (
                  sortDirection === 'desc' ? <ArrowDownIcon className="w-3 h-3" /> : <ArrowUpIcon className="w-3 h-3" />
                )}
              </button>
              <button
                onClick={() => handleSortClick('price')}
                className={`flex items-center gap-0.5 px-1.5 py-1 rounded text-xs ${
                  sortField === 'price'
                    ? 'bg-blue-100 text-blue-700'
                    : 'bg-gray-100 text-gray-600 hover:bg-gray-200'
                }`}
                title={`Sort by price ${sortField === 'price' ? (sortDirection === 'asc' ? '(lowest first)' : '(highest first)') : ''}`}
              >
                <DollarIcon className="w-3.5 h-3.5" />
                {sortField === 'price' && (
                  sortDirection === 'asc' ? <ArrowUpIcon className="w-3 h-3" /> : <ArrowDownIcon className="w-3 h-3" />
                )}
              </button>
            </div>
          </div>
          <div className="space-y-2">
            {sortedOutliers.slice(0, 10).map((outlier) => (
              <div
                key={outlier.id}
                onClick={() => onSelect(outlier)}
                onMouseEnter={() => onHover(outlier.id)}
                onMouseLeave={() => onHover(null)}
                className="relative p-2 rounded border border-gray-200 hover:border-gray-300 cursor-pointer bg-white hover:bg-gray-50"
              >
                <div className="flex justify-between items-start">
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium truncate">{outlier.address}</div>
                    <div className="text-sm text-gray-600">
                      ${outlier.price.toLocaleString()}
                      {outlier.bedrooms && ` · ${outlier.bedrooms} bd`}
                    </div>
                  </div>
                  {outlier.amenity_score != null && (
                    <div className={`text-xs font-semibold px-2 py-1 rounded ml-2 flex-shrink-0 ${getScoreBadgeClasses(outlier.amenity_score)}`}>
                      {Math.round(outlier.amenity_score)}
                    </div>
                  )}
                </div>
                {selectedListId !== null && onRemoveListing && (
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      onRemoveListing(outlier.id);
                    }}
                    className="absolute top-1 right-1 p-1 text-gray-400 hover:text-red-600 bg-white rounded-full shadow"
                    title="Remove from list"
                  >
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                )}
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

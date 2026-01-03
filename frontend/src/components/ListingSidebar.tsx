import type { Listing } from '../types';
import ListingCard from './ListingCard';

interface ListingSidebarProps {
  newListings: Listing[];
  allListings: Listing[];
  onSelect: (listing: Listing) => void;
}

export default function ListingSidebar({
  newListings,
  allListings,
  onSelect,
}: ListingSidebarProps) {
  return (
    <div className="p-4">
      {newListings.length > 0 && (
        <div className="mb-6">
          <h2 className="text-sm font-semibold text-gray-500 uppercase mb-2">
            New Since Last Visit ({newListings.length})
          </h2>
          <div className="space-y-2">
            {newListings.slice(0, 5).map((listing) => (
              <ListingCard
                key={listing.id}
                listing={listing}
                onClick={() => onSelect(listing)}
                compact
              />
            ))}
          </div>
        </div>
      )}

      <div>
        <h2 className="text-sm font-semibold text-gray-500 uppercase mb-2">
          All Listings ({allListings.length})
        </h2>
        <div className="space-y-2">
          {allListings.map((listing) => (
            <ListingCard
              key={listing.id}
              listing={listing}
              onClick={() => onSelect(listing)}
              compact
            />
          ))}
        </div>
      </div>
    </div>
  );
}

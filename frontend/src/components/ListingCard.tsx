import type { Listing } from '../types';
import { getScoreBadgeClasses } from '../utils/scoreColors';

interface ListingCardProps {
  listing: Listing;
  onClick: () => void;
  compact?: boolean;
}

export default function ListingCard({ listing, onClick, compact }: ListingCardProps) {
  const score = listing.amenity_score?.amenity_score;

  return (
    <div
      onClick={onClick}
      className={`
        p-3 bg-white rounded border cursor-pointer hover:border-blue-300 hover:shadow
        ${listing.is_new ? 'border-l-4 border-l-green-500' : ''}
        ${listing.is_favorite ? 'bg-yellow-50' : ''}
      `}
    >
      <div className="flex justify-between items-start">
        <div className="flex-1 min-w-0">
          <div className="font-medium truncate">{listing.address}</div>
          <div className="text-sm text-gray-600">
            ${listing.price.toLocaleString()}
            {!compact && listing.city && ` - ${listing.city}`}
          </div>
        </div>
        {score !== null && score !== undefined && (
          <div className={`text-xs font-semibold px-2 py-1 rounded ml-2 ${getScoreBadgeClasses(score)}`}>
            {score}
          </div>
        )}
      </div>

      {!compact && (
        <div className="mt-1 text-xs text-gray-500">
          {listing.bedrooms && `${listing.bedrooms} bed`}
          {listing.bathrooms && ` · ${listing.bathrooms} bath`}
          {listing.sqft && ` · ${listing.sqft.toLocaleString()} sqft`}
        </div>
      )}

      {listing.is_favorite && (
        <span className="text-yellow-500 text-xs">★ Favorite</span>
      )}
    </div>
  );
}

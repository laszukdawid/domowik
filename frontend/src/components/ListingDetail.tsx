import { useState } from 'react';
import type { Listing } from '../types';
import { useUpdateStatus, useNotes, useCreateNote } from '../hooks/useListings';

interface ListingDetailProps {
  listing: Listing;
  onClose: () => void;
}

export default function ListingDetail({ listing, onClose }: ListingDetailProps) {
  const [noteText, setNoteText] = useState('');
  const updateStatus = useUpdateStatus();
  const createNote = useCreateNote();
  const { data: notes = [] } = useNotes(listing.id);

  const handleFavorite = () => {
    updateStatus.mutate({
      listingId: listing.id,
      status: { is_favorite: !listing.is_favorite },
    });
  };

  const handleHide = () => {
    updateStatus.mutate({
      listingId: listing.id,
      status: { is_hidden: true },
    });
    onClose();
  };

  const handleAddNote = () => {
    if (!noteText.trim()) return;
    createNote.mutate(
      { listingId: listing.id, note: noteText },
      { onSuccess: () => setNoteText('') }
    );
  };

  const amenity = listing.amenity_score;

  return (
    <div className="p-4">
      <button
        onClick={onClose}
        className="text-sm text-gray-500 hover:text-gray-700 mb-4"
      >
        ← Back to list
      </button>

      <h2 className="text-lg font-semibold">{listing.address}</h2>
      <div className="text-xl font-bold text-green-600 mb-2">
        ${listing.price.toLocaleString()}
      </div>

      <div className="text-sm text-gray-600 mb-4">
        {listing.bedrooms && `${listing.bedrooms} bed`}
        {listing.bathrooms && ` · ${listing.bathrooms} bath`}
        {listing.sqft && ` · ${listing.sqft.toLocaleString()} sqft`}
        {listing.property_type && ` · ${listing.property_type}`}
      </div>

      {/* Amenity scores */}
      {amenity && (
        <div className="mb-4 p-3 bg-gray-50 rounded">
          <div className="text-sm font-medium mb-2">Amenities</div>
          <div className="space-y-1 text-sm">
            {amenity.nearest_park_m && (
              <div>🌳 Park: {amenity.nearest_park_m}m</div>
            )}
            {amenity.nearest_coffee_m && (
              <div>☕ Coffee: {amenity.nearest_coffee_m}m</div>
            )}
            {amenity.nearest_dog_park_m && (
              <div>🐕 Dog Park: {amenity.nearest_dog_park_m}m</div>
            )}
            {amenity.walkability_score && (
              <div className="font-medium mt-2">
                Walkability: {amenity.walkability_score}/100
              </div>
            )}
          </div>
        </div>
      )}

      {/* Actions */}
      <div className="flex gap-2 mb-4">
        <button
          onClick={handleFavorite}
          className={`flex-1 py-2 rounded text-sm ${
            listing.is_favorite
              ? 'bg-yellow-100 text-yellow-800'
              : 'bg-gray-100 hover:bg-gray-200'
          }`}
        >
          {listing.is_favorite ? '★ Favorited' : '☆ Favorite'}
        </button>
        <button
          onClick={handleHide}
          className="flex-1 py-2 rounded text-sm bg-gray-100 hover:bg-gray-200"
        >
          Hide
        </button>
        <a
          href={listing.url}
          target="_blank"
          rel="noopener noreferrer"
          className="flex-1 py-2 rounded text-sm bg-blue-100 text-blue-800 text-center hover:bg-blue-200"
        >
          View →
        </a>
      </div>

      {/* Notes */}
      <div className="border-t pt-4">
        <div className="text-sm font-medium mb-2">Notes</div>
        <div className="space-y-2 mb-3">
          {notes.map((note) => (
            <div key={note.id} className="p-2 bg-gray-50 rounded text-sm">
              <div className="font-medium text-xs text-gray-500">
                {note.user_name}
              </div>
              <div>{note.note}</div>
            </div>
          ))}
        </div>
        <div className="flex gap-2">
          <input
            type="text"
            value={noteText}
            onChange={(e) => setNoteText(e.target.value)}
            placeholder="Add a note..."
            className="flex-1 p-2 border rounded text-sm"
            onKeyDown={(e) => e.key === 'Enter' && handleAddNote()}
          />
          <button
            onClick={handleAddNote}
            disabled={!noteText.trim()}
            className="px-3 py-2 bg-blue-600 text-white rounded text-sm hover:bg-blue-700 disabled:opacity-50"
          >
            Add
          </button>
        </div>
      </div>
    </div>
  );
}

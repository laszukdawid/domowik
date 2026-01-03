import { useState } from 'react';
import { useAuth } from '../hooks/useAuth';
import { useListings } from '../hooks/useListings';
import type { Listing, ListingFilters } from '../types';
import Map from '../components/Map';
import ListingSidebar from '../components/ListingSidebar';
import ListingDetail from '../components/ListingDetail';
import FilterBar from '../components/FilterBar';

export default function Dashboard() {
  const { user, logout } = useAuth();
  const [filters, setFilters] = useState<ListingFilters>({});
  const [selectedListing, setSelectedListing] = useState<Listing | null>(null);

  const { data: listings = [], isLoading } = useListings(filters);

  const newListings = listings.filter((l) => l.is_new);
  const allListings = listings;

  return (
    <div className="h-screen flex flex-col">
      {/* Header */}
      <header className="bg-white shadow px-4 py-3 flex items-center justify-between">
        <h1 className="text-xl font-bold">HomeHero</h1>
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
          {isLoading ? (
            <div className="absolute inset-0 flex items-center justify-center bg-gray-100">
              Loading...
            </div>
          ) : (
            <Map
              listings={allListings}
              selectedId={selectedListing?.id}
              onSelect={setSelectedListing}
            />
          )}
        </div>

        {/* Sidebar */}
        <div className="w-80 bg-white border-l overflow-y-auto">
          {selectedListing ? (
            <ListingDetail
              listing={selectedListing}
              onClose={() => setSelectedListing(null)}
            />
          ) : (
            <ListingSidebar
              newListings={newListings}
              allListings={allListings}
              onSelect={setSelectedListing}
            />
          )}
        </div>
      </div>
    </div>
  );
}

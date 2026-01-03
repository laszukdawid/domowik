import { useState } from 'react';
import type { ListingFilters } from '../types';

interface FilterBarProps {
  filters: ListingFilters;
  onChange: (filters: ListingFilters) => void;
}

export default function FilterBar({ filters, onChange }: FilterBarProps) {
  const [isOpen, setIsOpen] = useState(false);

  const handleChange = (key: keyof ListingFilters, value: unknown) => {
    onChange({ ...filters, [key]: value || undefined });
  };

  const activeCount = Object.values(filters).filter(
    (v) => v !== undefined && v !== false
  ).length;

  return (
    <div className="relative">
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="px-3 py-1 bg-gray-100 rounded hover:bg-gray-200 text-sm flex items-center gap-2"
      >
        Filters
        {activeCount > 0 && (
          <span className="bg-blue-500 text-white px-1.5 rounded-full text-xs">
            {activeCount}
          </span>
        )}
      </button>

      {isOpen && (
        <div className="absolute right-0 top-full mt-2 w-72 bg-white rounded-lg shadow-lg border p-4 z-50">
          <div className="space-y-4">
            <div>
              <label className="block text-sm font-medium mb-1">Price Range</label>
              <div className="flex gap-2">
                <input
                  type="number"
                  placeholder="Min"
                  value={filters.min_price || ''}
                  onChange={(e) =>
                    handleChange('min_price', e.target.value ? Number(e.target.value) : undefined)
                  }
                  className="w-full p-2 border rounded text-sm"
                />
                <input
                  type="number"
                  placeholder="Max"
                  value={filters.max_price || ''}
                  onChange={(e) =>
                    handleChange('max_price', e.target.value ? Number(e.target.value) : undefined)
                  }
                  className="w-full p-2 border rounded text-sm"
                />
              </div>
            </div>

            <div>
              <label className="block text-sm font-medium mb-1">Min Bedrooms</label>
              <select
                value={filters.min_bedrooms || ''}
                onChange={(e) =>
                  handleChange('min_bedrooms', e.target.value ? Number(e.target.value) : undefined)
                }
                className="w-full p-2 border rounded text-sm"
              >
                <option value="">Any</option>
                <option value="1">1+</option>
                <option value="2">2+</option>
                <option value="3">3+</option>
                <option value="4">4+</option>
                <option value="5">5+</option>
              </select>
            </div>

            <div>
              <label className="block text-sm font-medium mb-1">Min Sqft</label>
              <input
                type="number"
                placeholder="Min sqft"
                value={filters.min_sqft || ''}
                onChange={(e) =>
                  handleChange('min_sqft', e.target.value ? Number(e.target.value) : undefined)
                }
                className="w-full p-2 border rounded text-sm"
              />
            </div>

            <div className="flex gap-4">
              <label className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={filters.favorites_only || false}
                  onChange={(e) => handleChange('favorites_only', e.target.checked)}
                />
                Favorites only
              </label>
              <label className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={filters.include_hidden || false}
                  onChange={(e) => handleChange('include_hidden', e.target.checked)}
                />
                Show hidden
              </label>
            </div>

            <button
              onClick={() => onChange({})}
              className="w-full py-2 text-sm text-gray-600 hover:text-gray-800"
            >
              Clear all filters
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

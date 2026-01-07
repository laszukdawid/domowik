import { useState } from 'react';
import type { FilterGroups, FilterGroup } from '../types';

interface FilterBarProps {
  filterGroups: FilterGroups;
  onChange: (filterGroups: FilterGroups) => void;
}

function FilterGroupForm({
  group,
  onChange,
  onRemove,
  showRemove,
}: {
  group: FilterGroup;
  onChange: (group: FilterGroup) => void;
  onRemove: () => void;
  showRemove: boolean;
}) {
  const handleChange = (key: keyof FilterGroup, value: unknown) => {
    onChange({ ...group, [key]: value || undefined });
  };

  return (
    <div className="p-3 border rounded bg-gray-50 space-y-3">
      <div className="flex justify-between items-center">
        <span className="text-xs font-semibold text-gray-600">Filter Group</span>
        {showRemove && (
          <button
            onClick={onRemove}
            className="text-red-500 hover:text-red-700 text-xs"
          >
            Remove
          </button>
        )}
      </div>

      <div>
        <label className="block text-xs font-medium mb-1">Price Range</label>
        <div className="flex gap-2">
          <input
            type="number"
            placeholder="Min"
            value={group.min_price || ''}
            onChange={(e) =>
              handleChange('min_price', e.target.value ? Number(e.target.value) : undefined)
            }
            className="w-full p-1.5 border rounded text-xs"
          />
          <input
            type="number"
            placeholder="Max"
            value={group.max_price || ''}
            onChange={(e) =>
              handleChange('max_price', e.target.value ? Number(e.target.value) : undefined)
            }
            className="w-full p-1.5 border rounded text-xs"
          />
        </div>
      </div>

      <div>
        <label className="block text-xs font-medium mb-1">Min Bedrooms</label>
        <select
          value={group.min_bedrooms || ''}
          onChange={(e) =>
            handleChange('min_bedrooms', e.target.value ? Number(e.target.value) : undefined)
          }
          className="w-full p-1.5 border rounded text-xs"
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
        <label className="block text-xs font-medium mb-1">Min Sqft</label>
        <input
          type="number"
          placeholder="Min sqft"
          value={group.min_sqft || ''}
          onChange={(e) =>
            handleChange('min_sqft', e.target.value ? Number(e.target.value) : undefined)
          }
          className="w-full p-1.5 border rounded text-xs"
        />
      </div>

      <div>
        <label className="block text-xs font-medium mb-1">Min Score</label>
        <select
          value={group.min_score ?? ''}
          onChange={(e) =>
            handleChange('min_score', e.target.value ? Number(e.target.value) : undefined)
          }
          className="w-full p-1.5 border rounded text-xs"
        >
          <option value="">Any (include 0)</option>
          <option value="1">1+ (exclude 0)</option>
          <option value="20">20+</option>
          <option value="40">40+</option>
          <option value="60">60+</option>
          <option value="80">80+</option>
        </select>
      </div>
    </div>
  );
}

export default function FilterBar({ filterGroups, onChange }: FilterBarProps) {
  const [isOpen, setIsOpen] = useState(false);

  const handleGroupChange = (index: number, group: FilterGroup) => {
    const newGroups = [...filterGroups.groups];
    newGroups[index] = group;
    onChange({ ...filterGroups, groups: newGroups });
  };

  const handleAddGroup = () => {
    onChange({
      ...filterGroups,
      groups: [...filterGroups.groups, {}],
    });
  };

  const handleRemoveGroup = (index: number) => {
    const newGroups = filterGroups.groups.filter((_, i) => i !== index);
    onChange({
      ...filterGroups,
      groups: newGroups.length > 0 ? newGroups : [{}],
    });
  };

  const handleGlobalChange = (key: 'include_hidden' | 'favorites_only', value: boolean) => {
    onChange({ ...filterGroups, [key]: value });
  };

  const activeCount = filterGroups.groups.reduce((count, group) => {
    return count + Object.values(group).filter((v) => v !== undefined).length;
  }, 0) + (filterGroups.include_hidden ? 1 : 0) + (filterGroups.favorites_only ? 1 : 0);

  const clearAll = () => {
    onChange({
      groups: [{}],
      include_hidden: false,
      favorites_only: false,
    });
  };

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
        <div className="absolute right-0 top-full mt-2 w-80 bg-white rounded-lg shadow-lg border p-4 z-[1000] max-h-[80vh] overflow-y-auto">
          <div className="space-y-3">
            <div className="flex justify-between items-center mb-2">
              <h3 className="text-sm font-semibold">Filter Groups (OR logic)</h3>
            </div>

            <p className="text-xs text-gray-600 mb-3">
              Add multiple filter groups. Listings matching ANY group will be shown.
            </p>

            {filterGroups.groups.map((group, index) => (
              <div key={index}>
                {index > 0 && (
                  <div className="text-center text-xs font-bold text-blue-600 my-2">
                    OR
                  </div>
                )}
                <FilterGroupForm
                  group={group}
                  onChange={(g) => handleGroupChange(index, g)}
                  onRemove={() => handleRemoveGroup(index)}
                  showRemove={filterGroups.groups.length > 1}
                />
              </div>
            ))}

            <button
              onClick={handleAddGroup}
              className="w-full py-2 text-sm text-blue-600 hover:text-blue-800 border border-blue-300 rounded hover:bg-blue-50"
            >
              + Add Filter Group
            </button>

            <div className="border-t pt-3 space-y-2">
              <h4 className="text-xs font-semibold text-gray-700">Global Filters</h4>
              <div className="flex gap-4">
                <label className="flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={filterGroups.favorites_only || false}
                    onChange={(e) => handleGlobalChange('favorites_only', e.target.checked)}
                  />
                  Favorites only
                </label>
                <label className="flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={filterGroups.include_hidden || false}
                    onChange={(e) => handleGlobalChange('include_hidden', e.target.checked)}
                  />
                  Show hidden
                </label>
              </div>
            </div>

            <button
              onClick={clearAll}
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

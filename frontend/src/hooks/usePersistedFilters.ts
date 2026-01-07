import { useState, useCallback } from 'react';
import { useSearchParams } from 'react-router-dom';
import { FilterGroups, FilterGroup } from '../types';

const FILTERS_STORAGE_KEY = 'domowik_filter_groups';

/**
 * Custom hook that persists filter groups to localStorage and URL search params
 * This ensures filters are remembered between sessions and URLs are shareable
 */
export function usePersistedFilters() {
  const [searchParams, setSearchParams] = useSearchParams();

  // Initialize filter groups from URL params first, then localStorage, then defaults
  const [filterGroups, setFilterGroupsState] = useState<FilterGroups>(() => {
    // Try URL params first (for shareable links)
    const urlFilters = parseFilterGroupsFromURL(searchParams);
    if (urlFilters && urlFilters.groups.length > 0) {
      return urlFilters;
    }

    // Fall back to localStorage
    try {
      const stored = localStorage.getItem(FILTERS_STORAGE_KEY);
      if (stored) {
        return JSON.parse(stored);
      }
    } catch (error) {
      console.error('Failed to load filter groups from localStorage:', error);
    }

    return { groups: [{ min_score: 1 }], include_hidden: false, favorites_only: false };
  });

  // Update both localStorage and URL params when filters change
  const setFilterGroups = useCallback((newFilters: FilterGroups | ((prev: FilterGroups) => FilterGroups)) => {
    setFilterGroupsState((prevFilters) => {
      const updatedFilters = typeof newFilters === 'function' ? newFilters(prevFilters) : newFilters;

      // Persist to localStorage
      try {
        localStorage.setItem(FILTERS_STORAGE_KEY, JSON.stringify(updatedFilters));
      } catch (error) {
        console.error('Failed to save filter groups to localStorage:', error);
      }

      // Update URL params
      updateURLParams(updatedFilters, setSearchParams);

      return updatedFilters;
    });
  }, [setSearchParams]);

  return [filterGroups, setFilterGroups] as const;
}

/**
 * Parse filter groups from URL search params
 */
function parseFilterGroupsFromURL(searchParams: URLSearchParams): FilterGroups | null {
  const groupsParam = searchParams.get('filter_groups');

  if (groupsParam) {
    try {
      return JSON.parse(decodeURIComponent(groupsParam));
    } catch (error) {
      console.error('Failed to parse filter_groups from URL:', error);
    }
  }

  // Fallback: try to parse legacy format (single filter group)
  const group: FilterGroup = {};
  let hasFilters = false;

  const minPrice = searchParams.get('min_price');
  if (minPrice) {
    group.min_price = Number(minPrice);
    hasFilters = true;
  }

  const maxPrice = searchParams.get('max_price');
  if (maxPrice) {
    group.max_price = Number(maxPrice);
    hasFilters = true;
  }

  const minBedrooms = searchParams.get('min_bedrooms');
  if (minBedrooms) {
    group.min_bedrooms = Number(minBedrooms);
    hasFilters = true;
  }

  const minSqft = searchParams.get('min_sqft');
  if (minSqft) {
    group.min_sqft = Number(minSqft);
    hasFilters = true;
  }

  const minScore = searchParams.get('min_score');
  if (minScore) {
    group.min_score = Number(minScore);
    hasFilters = true;
  }

  const cities = searchParams.getAll('cities');
  if (cities.length > 0) {
    group.cities = cities;
    hasFilters = true;
  }

  const propertyTypes = searchParams.getAll('property_types');
  if (propertyTypes.length > 0) {
    group.property_types = propertyTypes;
    hasFilters = true;
  }

  if (!hasFilters) {
    return null;
  }

  const includeHidden = searchParams.get('include_hidden');
  const favoritesOnly = searchParams.get('favorites_only');

  return {
    groups: [group],
    include_hidden: includeHidden === 'true',
    favorites_only: favoritesOnly === 'true',
  };
}

/**
 * Update URL search params with current filter groups
 */
function updateURLParams(filterGroups: FilterGroups, setSearchParams: (params: URLSearchParams) => void) {
  const params = new URLSearchParams();

  // Serialize filter groups as a single JSON parameter
  const groupsJson = JSON.stringify(filterGroups);
  params.append('filter_groups', encodeURIComponent(groupsJson));

  setSearchParams(params);
}

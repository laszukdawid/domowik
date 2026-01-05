import { useState, useCallback } from 'react';
import { useSearchParams } from 'react-router-dom';
import { ListingFilters } from '../types';

const FILTERS_STORAGE_KEY = 'domowik_filters';

/**
 * Custom hook that persists filters to localStorage and URL search params
 * This ensures filters are remembered between sessions and URLs are shareable
 */
export function usePersistedFilters() {
  const [searchParams, setSearchParams] = useSearchParams();

  // Initialize filters from URL params first, then localStorage, then defaults
  const [filters, setFiltersState] = useState<ListingFilters>(() => {
    // Try URL params first (for shareable links)
    const urlFilters = parseFiltersFromURL(searchParams);
    if (Object.keys(urlFilters).length > 0) {
      return urlFilters;
    }

    // Fall back to localStorage
    try {
      const stored = localStorage.getItem(FILTERS_STORAGE_KEY);
      if (stored) {
        return JSON.parse(stored);
      }
    } catch (error) {
      console.error('Failed to load filters from localStorage:', error);
    }

    return { min_score: 1 };
  });

  // Update both localStorage and URL params when filters change
  const setFilters = useCallback((newFilters: ListingFilters | ((prev: ListingFilters) => ListingFilters)) => {
    setFiltersState((prevFilters) => {
      const updatedFilters = typeof newFilters === 'function' ? newFilters(prevFilters) : newFilters;

      // Persist to localStorage
      try {
        if (Object.keys(updatedFilters).length === 0) {
          localStorage.removeItem(FILTERS_STORAGE_KEY);
        } else {
          localStorage.setItem(FILTERS_STORAGE_KEY, JSON.stringify(updatedFilters));
        }
      } catch (error) {
        console.error('Failed to save filters to localStorage:', error);
      }

      // Update URL params
      updateURLParams(updatedFilters, setSearchParams);

      return updatedFilters;
    });
  }, [setSearchParams]);

  return [filters, setFilters] as const;
}

/**
 * Parse filters from URL search params
 */
function parseFiltersFromURL(searchParams: URLSearchParams): ListingFilters {
  const filters: ListingFilters = {};

  const minPrice = searchParams.get('min_price');
  if (minPrice) filters.min_price = Number(minPrice);

  const maxPrice = searchParams.get('max_price');
  if (maxPrice) filters.max_price = Number(maxPrice);

  const minBedrooms = searchParams.get('min_bedrooms');
  if (minBedrooms) filters.min_bedrooms = Number(minBedrooms);

  const minSqft = searchParams.get('min_sqft');
  if (minSqft) filters.min_sqft = Number(minSqft);

  const cities = searchParams.getAll('cities');
  if (cities.length > 0) filters.cities = cities;

  const propertyTypes = searchParams.getAll('property_types');
  if (propertyTypes.length > 0) filters.property_types = propertyTypes;

  const includeHidden = searchParams.get('include_hidden');
  if (includeHidden) filters.include_hidden = includeHidden === 'true';

  const favoritesOnly = searchParams.get('favorites_only');
  if (favoritesOnly) filters.favorites_only = favoritesOnly === 'true';

  const minScore = searchParams.get('min_score');
  if (minScore) filters.min_score = Number(minScore);

  return filters;
}

/**
 * Update URL search params with current filters
 */
function updateURLParams(filters: ListingFilters, setSearchParams: (params: URLSearchParams) => void) {
  const params = new URLSearchParams();

  Object.entries(filters).forEach(([key, value]) => {
    if (value !== undefined && value !== null) {
      if (Array.isArray(value)) {
        value.forEach((v) => params.append(key, String(v)));
      } else {
        params.append(key, String(value));
      }
    }
  });

  setSearchParams(params);
}

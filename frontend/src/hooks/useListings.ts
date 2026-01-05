import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useState, useEffect, useRef } from 'react';
import { api } from '../api/client';
import type { ListingFilters, Listing } from '../types';

/**
 * Hook for streaming listings with progressive loading
 * Returns listings as they arrive from the server for a snappier UX
 *
 * Only fetches when bbox is provided to avoid fetching all listings.
 */
export function useListings(filters: ListingFilters = {}) {
  const [streamedListings, setStreamedListings] = useState<Listing[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const [streamError, setStreamError] = useState<Error | null>(null);
  const abortControllerRef = useRef<AbortController | null>(null);

  // Serialize filters to a stable string for comparison
  const filtersKey = JSON.stringify(filters);

  useEffect(() => {
    // Don't fetch if no bbox provided - would fetch all listings
    if (!filters.bbox) {
      setStreamedListings([]);
      setIsStreaming(false);
      return;
    }

    // Abort previous stream if filters changed
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }

    // Create new abort controller for this stream
    abortControllerRef.current = new AbortController();
    const currentFiltersKey = filtersKey;

    setIsStreaming(true);
    setStreamError(null);
    setStreamedListings([]); // Clear previous results

    api.streamListings(
      filters,
      (chunk) => {
        // Only update if filters haven't changed
        setStreamedListings((prev) => [...prev, ...chunk]);
      },
      () => {
        setIsStreaming(false);
      },
      (error) => {
        // Only update if not aborted
        if (error.name !== 'AbortError') {
          setStreamError(error);
          setIsStreaming(false);
        }
      }
    );

    return () => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filtersKey]);

  return {
    data: streamedListings,
    isLoading: isStreaming && streamedListings.length === 0,
    isStreaming,
    error: streamError,
  };
}

/**
 * Legacy hook using React Query (non-streaming)
 * Kept for compatibility if needed
 */
export function useListingsLegacy(filters: ListingFilters = {}) {
  return useQuery({
    queryKey: ['listings', filters],
    queryFn: () => api.getListings(filters),
    placeholderData: (previousData) => previousData, // Keep showing previous results while fetching
  });
}

export function useListing(id: number) {
  return useQuery({
    queryKey: ['listing', id],
    queryFn: () => api.getListing(id),
    enabled: !!id,
  });
}

export function useUpdateStatus() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      listingId,
      status,
    }: {
      listingId: number;
      status: { is_favorite?: boolean; is_hidden?: boolean };
    }) => api.updateStatus(listingId, status),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['listings'] });
    },
  });
}

export function useNotes(listingId: number) {
  return useQuery({
    queryKey: ['notes', listingId],
    queryFn: () => api.getNotes(listingId),
    enabled: !!listingId,
  });
}

export function useCreateNote() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ listingId, note }: { listingId: number; note: string }) =>
      api.createNote(listingId, note),
    onSuccess: (_, { listingId }) => {
      queryClient.invalidateQueries({ queryKey: ['notes', listingId] });
    },
  });
}

export function useDeleteNote() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ listingId, noteId }: { listingId: number; noteId: number }) =>
      api.deleteNote(listingId, noteId),
    onSuccess: (_, { listingId }) => {
      queryClient.invalidateQueries({ queryKey: ['notes', listingId] });
    },
  });
}

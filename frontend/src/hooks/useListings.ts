import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useState, useEffect, useRef } from 'react';
import { api } from '../api/client';
import type { ListingFilters, Listing } from '../types';

/**
 * Hook for streaming listings with progressive loading
 * Returns listings as they arrive from the server for a snappier UX
 */
export function useListings(filters: ListingFilters = {}) {
  const [streamedListings, setStreamedListings] = useState<Listing[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const [streamError, setStreamError] = useState<Error | null>(null);
  const abortControllerRef = useRef<AbortController | null>(null);
  const filtersRef = useRef(filters);

  // Update filters ref when filters change
  useEffect(() => {
    filtersRef.current = filters;
  }, [filters]);

  useEffect(() => {
    // Abort previous stream if filters changed
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }

    // Create new abort controller for this stream
    abortControllerRef.current = new AbortController();
    const currentFilters = filtersRef.current;

    setIsStreaming(true);
    setStreamError(null);
    setStreamedListings([]); // Clear previous results

    api.streamListings(
      currentFilters,
      (chunk) => {
        // Only update if filters haven't changed
        if (JSON.stringify(filtersRef.current) === JSON.stringify(currentFilters)) {
          setStreamedListings((prev) => [...prev, ...chunk]);
        }
      },
      () => {
        // Only update if filters haven't changed
        if (JSON.stringify(filtersRef.current) === JSON.stringify(currentFilters)) {
          setIsStreaming(false);
        }
      },
      (error) => {
        // Only update if filters haven't changed and not aborted
        if (
          JSON.stringify(filtersRef.current) === JSON.stringify(currentFilters) &&
          error.name !== 'AbortError'
        ) {
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
  }, [filters]);

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

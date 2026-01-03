import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api } from '../api/client';
import type { ListingFilters } from '../types';

export function useListings(filters: ListingFilters = {}) {
  return useQuery({
    queryKey: ['listings', filters],
    queryFn: () => api.getListings(filters),
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

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api } from '../api/client';

export function useCustomLists() {
  return useQuery({
    queryKey: ['customLists'],
    queryFn: () => api.getCustomLists(),
  });
}

export function useCreateCustomList() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (name?: string) => api.createCustomList(name),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['customLists'] });
    },
  });
}

export function useUpdateCustomList() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ id, name }: { id: number; name: string }) =>
      api.updateCustomList(id, name),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['customLists'] });
    },
  });
}

export function useDeleteCustomList() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: number) => api.deleteCustomList(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['customLists'] });
    },
  });
}

export function useAddListingToCustomList() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ listId, input }: { listId: number; input: string }) =>
      api.addListingToCustomList(listId, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['customLists'] });
      queryClient.invalidateQueries({ queryKey: ['clusters'] });
    },
  });
}

export function useRemoveListingFromCustomList() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ listId, listingId }: { listId: number; listingId: number }) =>
      api.removeListingFromCustomList(listId, listingId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['customLists'] });
      queryClient.invalidateQueries({ queryKey: ['clusters'] });
    },
  });
}

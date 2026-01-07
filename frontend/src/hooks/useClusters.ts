import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef } from 'react';
import { api } from '../api/client';
import type { FilterGroups, BBox, ClusterResponse } from '../types';

interface UseClusterParams {
  bbox: BBox | null;
  zoom: number;
  filterGroups: FilterGroups;
  enabled?: boolean;
}

export function useClusters({ bbox, zoom, filterGroups, enabled = true }: UseClusterParams) {
  const queryClient = useQueryClient();
  const previousQueryKeyRef = useRef<string | null>(null);

  const bboxString = bbox
    ? `${bbox.minLng},${bbox.minLat},${bbox.maxLng},${bbox.maxLat}`
    : null;

  // Serialize filters to stable string for queryKey
  const filtersKey = JSON.stringify(filterGroups);
  const currentQueryKey = `clusters-${bboxString}-${zoom}-${filtersKey}`;

  // Cancel previous query when a new one starts
  useEffect(() => {
    if (previousQueryKeyRef.current && previousQueryKeyRef.current !== currentQueryKey) {
      // Cancel any in-flight queries for the previous key
      queryClient.cancelQueries({
        queryKey: ['clusters'],
        exact: false,
      });
    }
    previousQueryKeyRef.current = currentQueryKey;
  }, [currentQueryKey, queryClient]);

  return useQuery<ClusterResponse>({
    queryKey: ['clusters', bboxString, zoom, filtersKey],
    queryFn: ({ signal }) => api.getClustersWithGroups(bboxString!, zoom, filterGroups, signal),
    enabled: enabled && !!bboxString,
    staleTime: 30000, // 30 seconds - clusters don't change often
    placeholderData: (prev) => prev, // Keep previous while fetching
  });
}

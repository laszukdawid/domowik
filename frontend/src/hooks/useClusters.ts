import { useQuery } from '@tanstack/react-query';
import { api } from '../api/client';
import type { ListingFilters, BBox, ClusterResponse } from '../types';

interface UseClusterParams {
  bbox: BBox | null;
  zoom: number;
  filters: ListingFilters;
  enabled?: boolean;
}

export function useClusters({ bbox, zoom, filters, enabled = true }: UseClusterParams) {
  const bboxString = bbox
    ? `${bbox.minLng},${bbox.minLat},${bbox.maxLng},${bbox.maxLat}`
    : null;

  return useQuery<ClusterResponse>({
    queryKey: ['clusters', bboxString, zoom, filters],
    queryFn: () => api.getClusters(bboxString!, zoom, filters),
    enabled: enabled && !!bboxString,
    staleTime: 30000, // 30 seconds - clusters don't change often
    placeholderData: (prev) => prev, // Keep previous while fetching
  });
}

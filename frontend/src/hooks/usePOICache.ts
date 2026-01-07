import { useRef, useCallback } from 'react';
import type { POI } from '../types';
import { api } from '../api/client';

interface CacheEntry {
  poi: POI;
  lastAccessed: number;
}

const MAX_CACHE_SIZE = 500;

export function usePOICache() {
  const cache = useRef<Map<number, CacheEntry>>(new Map());

  const get = useCallback((id: number): POI | undefined => {
    const entry = cache.current.get(id);
    if (entry) {
      entry.lastAccessed = Date.now();
      return entry.poi;
    }
    return undefined;
  }, []);

  const set = useCallback((poi: POI): void => {
    if (cache.current.size >= MAX_CACHE_SIZE) {
      // Evict LRU entry
      let oldestId: number | null = null;
      let oldestTime = Infinity;
      for (const [id, entry] of cache.current) {
        if (entry.lastAccessed < oldestTime) {
          oldestTime = entry.lastAccessed;
          oldestId = id;
        }
      }
      if (oldestId !== null) {
        cache.current.delete(oldestId);
      }
    }
    cache.current.set(poi.id, { poi, lastAccessed: Date.now() });
  }, []);

  const fetchPOIs = useCallback(async (ids: number[]): Promise<POI[]> => {
    const cached: POI[] = [];
    const missing: number[] = [];

    for (const id of ids) {
      const poi = get(id);
      if (poi) {
        cached.push(poi);
      } else {
        missing.push(id);
      }
    }

    if (missing.length > 0) {
      const fetched = await api.getPOIs(missing);
      for (const poi of fetched) {
        set(poi);
      }
      return [...cached, ...fetched];
    }

    return cached;
  }, [get, set]);

  return { get, fetchPOIs };
}

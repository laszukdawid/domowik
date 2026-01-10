import { useState, useCallback, useEffect } from 'react';

const SELECTED_LIST_STORAGE_KEY = 'domowik_selected_list_id';

/**
 * Custom hook that persists the selected custom list ID to localStorage
 * This ensures the selected list is remembered between sessions
 */
export function usePersistedListSelection() {
  const [selectedListId, setSelectedListIdState] = useState<number | null>(() => {
    try {
      const stored = localStorage.getItem(SELECTED_LIST_STORAGE_KEY);
      if (stored) {
        const parsed = JSON.parse(stored);
        return typeof parsed === 'number' ? parsed : null;
      }
    } catch (error) {
      console.error('Failed to load selected list from localStorage:', error);
    }
    return null;
  });

  const setSelectedListId = useCallback((id: number | null) => {
    setSelectedListIdState(id);
    try {
      if (id === null) {
        localStorage.removeItem(SELECTED_LIST_STORAGE_KEY);
      } else {
        localStorage.setItem(SELECTED_LIST_STORAGE_KEY, JSON.stringify(id));
      }
    } catch (error) {
      console.error('Failed to save selected list to localStorage:', error);
    }
  }, []);

  return [selectedListId, setSelectedListId] as const;
}

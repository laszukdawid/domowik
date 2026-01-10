import type { Listing, Note, Preferences, ListingFilters, ClusterResponse, POI, FilterGroups, CustomList, AddListingResult } from '../types';

const API_URL = import.meta.env.VITE_API_URL || 'http://localhost:8000';

class ApiClient {
  private token: string | null = null;

  constructor() {
    this.token = localStorage.getItem('token');
  }

  setToken(token: string | null) {
    this.token = token;
    if (token) {
      localStorage.setItem('token', token);
    } else {
      localStorage.removeItem('token');
    }
  }

  getToken(): string | null {
    return this.token;
  }

  private async request<T>(
    path: string,
    options: RequestInit = {},
    signal?: AbortSignal
  ): Promise<T> {
    const headers: HeadersInit = {
      'Content-Type': 'application/json',
      ...options.headers,
    };

    if (this.token) {
      (headers as Record<string, string>)['Authorization'] = `Bearer ${this.token}`;
    }

    const response = await fetch(`${API_URL}${path}`, {
      ...options,
      headers,
      signal,
    });

    if (!response.ok) {
      const error = await response.json().catch(() => ({ detail: 'Request failed' }));
      throw new Error(error.detail || 'Request failed');
    }

    return response.json();
  }

  // Auth
  async register(email: string, name: string, password: string) {
    const data = await this.request<{ access_token: string }>('/api/auth/register', {
      method: 'POST',
      body: JSON.stringify({ email, name, password }),
    });
    this.setToken(data.access_token);
    return data;
  }

  async login(email: string, password: string) {
    const data = await this.request<{ access_token: string }>('/api/auth/login', {
      method: 'POST',
      body: JSON.stringify({ email, password }),
    });
    this.setToken(data.access_token);
    return data;
  }

  async getMe() {
    return this.request<{ id: number; email: string; name: string }>('/api/auth/me');
  }

  logout() {
    this.setToken(null);
  }

  // Listings
  async getListings(filters: ListingFilters = {}) {
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
    const query = params.toString();
    return this.request<Listing[]>(`/api/listings${query ? `?${query}` : ''}`);
  }

  /**
   * Stream listings in chunks for progressive loading
   * @param filters - Listing filters
   * @param onChunk - Callback function that receives each chunk of listings
   * @param onComplete - Callback function called when streaming is complete
   * @param onError - Callback function called if an error occurs
   */
  async streamListings(
    filters: ListingFilters = {},
    onChunk: (listings: Listing[]) => void,
    onComplete?: () => void,
    onError?: (error: Error) => void
  ): Promise<void> {
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
    const query = params.toString();

    const headers: HeadersInit = {};
    if (this.token) {
      headers['Authorization'] = `Bearer ${this.token}`;
    }

    try {
      const response = await fetch(
        `${API_URL}/api/listings/stream${query ? `?${query}` : ''}`,
        { headers }
      );

      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }

      const reader = response.body?.getReader();
      if (!reader) {
        throw new Error('Response body is not readable');
      }

      const decoder = new TextDecoder();
      let buffer = '';

      while (true) {
        const { done, value } = await reader.read();

        if (done) {
          onComplete?.();
          break;
        }

        // Decode the chunk and add to buffer
        buffer += decoder.decode(value, { stream: true });

        // Process complete lines (chunks)
        const lines = buffer.split('\n');
        buffer = lines.pop() || ''; // Keep incomplete line in buffer

        for (const line of lines) {
          if (line.trim()) {
            try {
              const chunk = JSON.parse(line) as Listing[];
              onChunk(chunk);
            } catch (e) {
              console.error('Failed to parse chunk:', e);
            }
          }
        }
      }
    } catch (error) {
      onError?.(error instanceof Error ? error : new Error('Streaming failed'));
    }
  }

  async getListing(id: number) {
    return this.request<Listing>(`/api/listings/${id}`);
  }

  /**
   * Stream listings with OR filter groups in chunks for progressive loading
   */
  async streamListingsWithGroups(
    filterGroups: FilterGroups,
    bbox?: string,
    onChunk?: (listings: Listing[]) => void,
    onComplete?: () => void,
    onError?: (error: Error) => void
  ): Promise<void> {
    const params = new URLSearchParams();
    if (bbox) {
      params.append('bbox', bbox);
    }

    const headers: HeadersInit = {
      'Content-Type': 'application/json',
    };
    if (this.token) {
      headers['Authorization'] = `Bearer ${this.token}`;
    }

    try {
      const response = await fetch(
        `${API_URL}/api/listings/stream-groups${params.toString() ? `?${params.toString()}` : ''}`,
        {
          method: 'POST',
          headers,
          body: JSON.stringify(filterGroups),
        }
      );

      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }

      const reader = response.body?.getReader();
      if (!reader) {
        throw new Error('Response body is not readable');
      }

      const decoder = new TextDecoder();
      let buffer = '';

      while (true) {
        const { done, value } = await reader.read();

        if (done) {
          onComplete?.();
          break;
        }

        buffer += decoder.decode(value, { stream: true });

        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
          if (line.trim()) {
            try {
              const chunk = JSON.parse(line) as Listing[];
              onChunk?.(chunk);
            } catch (e) {
              console.error('Failed to parse chunk:', e);
            }
          }
        }
      }
    } catch (error) {
      onError?.(error instanceof Error ? error : new Error('Streaming failed'));
    }
  }

  // Status
  async updateStatus(listingId: number, status: { is_favorite?: boolean; is_hidden?: boolean }) {
    return this.request<{ is_favorite: boolean; is_hidden: boolean }>(
      `/api/listings/${listingId}/status`,
      {
        method: 'PUT',
        body: JSON.stringify(status),
      }
    );
  }

  // Notes
  async getNotes(listingId: number) {
    return this.request<Note[]>(`/api/listings/${listingId}/notes`);
  }

  async createNote(listingId: number, note: string) {
    return this.request<Note>(`/api/listings/${listingId}/notes`, {
      method: 'POST',
      body: JSON.stringify({ note }),
    });
  }

  async deleteNote(listingId: number, noteId: number) {
    return this.request(`/api/listings/${listingId}/notes/${noteId}`, {
      method: 'DELETE',
    });
  }

  // Preferences
  async getPreferences() {
    return this.request<Preferences>('/api/preferences');
  }

  async updatePreferences(prefs: Partial<Preferences>) {
    return this.request<Preferences>('/api/preferences', {
      method: 'PUT',
      body: JSON.stringify(prefs),
    });
  }

  // Clusters
  async getClusters(
    bbox: string,
    zoom: number,
    filters: ListingFilters = {},
    signal?: AbortSignal
  ): Promise<ClusterResponse> {
    const params = new URLSearchParams();
    params.append('bbox', bbox);
    params.append('zoom', String(zoom));

    Object.entries(filters).forEach(([key, value]) => {
      if (value !== undefined && value !== null && key !== 'bbox') {
        if (Array.isArray(value)) {
          value.forEach((v) => params.append(key, String(v)));
        } else {
          params.append(key, String(value));
        }
      }
    });

    return this.request<ClusterResponse>(`/api/clusters?${params.toString()}`, {}, signal);
  }

  async getClustersWithGroups(
    bbox: string,
    zoom: number,
    filterGroups: FilterGroups,
    signal?: AbortSignal
  ): Promise<ClusterResponse> {
    const params = new URLSearchParams();
    params.append('bbox', bbox);
    params.append('zoom', String(zoom));

    const headers: HeadersInit = {
      'Content-Type': 'application/json',
    };
    if (this.token) {
      headers['Authorization'] = `Bearer ${this.token}`;
    }

    const response = await fetch(
      `${API_URL}/api/clusters/groups?${params.toString()}`,
      {
        method: 'POST',
        headers,
        body: JSON.stringify(filterGroups),
        signal,
      }
    );

    if (!response.ok) {
      const error = await response.json().catch(() => ({ detail: 'Request failed' }));
      throw new Error(error.detail || 'Request failed');
    }

    return response.json();
  }

  // POIs
  async getPOIs(ids: number[]): Promise<POI[]> {
    if (ids.length === 0) return [];

    const params = new URLSearchParams();
    ids.forEach(id => params.append('ids', String(id)));

    return this.request<POI[]>(`/api/pois?${params.toString()}`);
  }

  // Custom Lists
  async getCustomLists(): Promise<CustomList[]> {
    return this.request<CustomList[]>('/api/custom-lists');
  }

  async createCustomList(name?: string): Promise<CustomList> {
    return this.request<CustomList>('/api/custom-lists', {
      method: 'POST',
      body: JSON.stringify(name ? { name } : {}),
    });
  }

  async updateCustomList(id: number, name: string): Promise<CustomList> {
    return this.request<CustomList>(`/api/custom-lists/${id}`, {
      method: 'PATCH',
      body: JSON.stringify({ name }),
    });
  }

  async deleteCustomList(id: number): Promise<void> {
    await this.request(`/api/custom-lists/${id}`, {
      method: 'DELETE',
    });
  }

  async addListingToCustomList(listId: number, input: string): Promise<AddListingResult> {
    return this.request<AddListingResult>(`/api/custom-lists/${listId}/listings`, {
      method: 'POST',
      body: JSON.stringify({ input }),
    });
  }

  async removeListingFromCustomList(listId: number, listingId: number): Promise<void> {
    await this.request(`/api/custom-lists/${listId}/listings/${listingId}`, {
      method: 'DELETE',
    });
  }
}

export const api = new ApiClient();

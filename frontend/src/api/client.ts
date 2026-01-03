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
    options: RequestInit = {}
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
  async getListings(filters: Record<string, unknown> = {}) {
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

  async getListing(id: number) {
    return this.request<Listing>(`/api/listings/${id}`);
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
}

// Import types
import type { Listing, Note, Preferences } from '../types';

export const api = new ApiClient();

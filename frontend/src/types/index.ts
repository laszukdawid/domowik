export interface AmenityScore {
  nearest_park_m: number | null;
  nearest_coffee_m: number | null;
  nearest_dog_park_m: number | null;
  parks: Array<{ name: string; distance_m: number }>;
  coffee_shops: Array<{ name: string; distance_m: number }>;
  walkability_score: number | null;
  amenity_score: number | null;
}

export interface Listing {
  id: number;
  mls_id: string;
  url: string;
  address: string;
  city: string;
  latitude: number | null;
  longitude: number | null;
  price: number;
  bedrooms: number | null;
  bathrooms: number | null;
  sqft: number | null;
  property_type: string | null;
  listing_date: string | null;
  first_seen: string;
  status: string;
  amenity_score: AmenityScore | null;
  is_favorite: boolean;
  is_hidden: boolean;
  is_new: boolean;
}

export interface Note {
  id: number;
  listing_id: number;
  user_id: number;
  user_name: string;
  note: string;
  created_at: string;
}

export interface User {
  id: number;
  email: string;
  name: string;
}

export interface Preferences {
  min_price: number | null;
  max_price: number | null;
  min_bedrooms: number | null;
  min_sqft: number | null;
  cities: string[] | null;
  property_types: string[] | null;
  max_park_distance: number | null;
  notify_email: boolean;
}

export interface ListingFilters {
  min_price?: number;
  max_price?: number;
  min_bedrooms?: number;
  min_sqft?: number;
  cities?: string[];
  property_types?: string[];
  include_hidden?: boolean;
  favorites_only?: boolean;
  min_score?: number;
  bbox?: string;
}

export interface ClusterStats {
  price_min: number;
  price_max: number;
  price_avg: number;
  beds_min: number | null;
  beds_max: number | null;
  amenity_avg: number | null;
}

export interface Cluster {
  id: string;
  label: string;
  center: { lat: number; lng: number };
  bounds: { north: number; south: number; east: number; west: number };
  count: number;
  stats: ClusterStats;
  listing_ids: number[];
}

export interface ClusterOutlier {
  id: number;
  lat: number;
  lng: number;
  price: number;
  bedrooms: number | null;
  address: string;
  amenity_score: number | null;
  is_favorite: boolean;
}

export interface ClusterResponse {
  clusters: Cluster[];
  outliers: ClusterOutlier[];
}

export interface BBox {
  minLng: number;
  minLat: number;
  maxLng: number;
  maxLat: number;
}

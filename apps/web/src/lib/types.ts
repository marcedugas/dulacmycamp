/** Mirrors the JSON the Rust API serialises. Dates are ISO strings. */

export type Role = 'guest' | 'admin';
export type BookingStatus = 'pending' | 'approved' | 'denied' | 'cancelled';

export interface User {
  id: string;
  email: string;
  full_name: string | null;
  phone: string | null;
  relationship: string | null;
  boat_info: string | null;
  notes: string | null;
  role: Role;
  /** Receives the booking approve/deny email. Any number of users may be owners. */
  is_owner: boolean;
  avatar_url: string | null;
  last_login_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface UserWithStats extends User {
  booking_count: number;
}

/**
 * A booking as the current caller may see it. Fields after `is_mine` are only
 * present for the booking's owner and for admins — the public calendar shows
 * that the camp is taken, never by whom.
 */
export interface Booking {
  id: string;
  check_in: string;
  check_out: string;
  status: BookingStatus;
  guest_count_adults: number;
  guest_count_kids: number;
  is_mine: boolean;
  user_id?: string;
  guest_name?: string;
  guest_email?: string;
  has_pets?: boolean;
  other_requests?: string;
  denied_reason?: string;
  approved_at?: string;
  approved_by?: string;
  created_at?: string;
}

export interface Capacity {
  approved_adults: number;
  total_adults: number;
  limit: number;
  over_capacity: boolean;
}

export interface CreateBookingResponse {
  booking: Booking;
  warning?: string;
  capacity: Capacity;
}

export interface BlackoutDate {
  id: string;
  start_date: string;
  end_date: string;
  reason: string | null;
  created_by: string | null;
  created_at: string;
}

export interface SpecialEvent {
  id: string;
  name: string;
  event_date: string;
  end_date: string | null;
  description: string | null;
  emoji: string | null;
  created_by: string | null;
  created_at: string;
}

/**
 * A US federal holiday, computed server-side for a given year — not an
 * admin-created row like {@link SpecialEvent}. Reference-only: it carries no
 * booking meaning and never affects availability or capacity.
 */
export interface Holiday {
  date: string;
  name: string;
}

// ── site content ──

/** Public, combined payload for the landing page — GET /api/site-content. */
export interface SiteContent {
  hero_title: string;
  hero_subtitle: string;
  about_text: string;
  hero_image_url: string | null;
  guest_photos_url: string | null;
  rules: { id: string; text: string }[];
  amenities: { id: string; label: string; icon: string | null }[];
  gallery: { id: string; url: string; caption: string | null }[];
}

export interface RuleItem {
  id: string;
  text: string;
  sort_order: number;
  created_at: string;
}

export interface AmenityItem {
  id: string;
  label: string;
  icon: string | null;
  sort_order: number;
  created_at: string;
}

export interface GalleryPhoto {
  id: string;
  url: string;
  caption: string | null;
  sort_order: number;
  created_at: string;
}

export interface Message {
  id: string;
  subject: string | null;
  body: string;
  is_read: boolean;
  booking_id: string | null;
  created_at: string;
  sender_id: string | null;
  sender_name: string | null;
  sender_email: string | null;
  recipient_id: string;
  recipient_name: string | null;
  recipient_email: string;
}

export interface AuthResponse {
  token: string;
  user: User;
}

// ── environment feeds ──

export interface WeatherCurrent {
  temp_f: number | null;
  conditions: string | null;
  wind_mph: number | null;
  wind_gust_mph?: number | null;
  wind_text?: string | null;
  wind_direction_deg?: number | null;
  wind_direction?: string | null;
  humidity: number | null;
  observed_at: string | null;
  source: 'observation' | 'forecast';
}

export interface ForecastDay {
  name: string;
  start_time: string;
  high_f: number | null;
  low_f: number | null;
  short_forecast: string;
  detailed_forecast: string;
  wind: string | null;
  wind_direction: string | null;
  precip_chance: number | null;
  icon: string | null;
}

export interface Weather {
  location: string;
  current: WeatherCurrent | null;
  forecast: ForecastDay[];
  updated_at: string;
}

export interface TidePrediction {
  /** Local time at the station, "YYYY-MM-DD HH:MM". */
  time: string;
  height_ft: number | null;
  kind: 'high' | 'low' | 'unknown';
}

export interface Tides {
  station_id: string;
  station_name: string | null;
  timezone: string;
  predictions: TidePrediction[];
  updated_at: string;
}

export interface LunarDay {
  date: string;
  phase: string;
  emoji: string;
  fraction: number;
  illumination: number;
  sunrise: string | null;
  sunset: string | null;
  sunrise_local: string | null;
  sunset_local: string | null;
}

export interface Lunar {
  location: string;
  timezone: string;
  current: { date: string; phase: string; emoji: string; illumination: number };
  next_full_moon: { at: string; date: string; days_away: number };
  days: LunarDay[];
}

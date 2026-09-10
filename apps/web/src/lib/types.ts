/** Mirrors the JSON the Rust API serialises. Dates are ISO strings. */

/**
 * The roles an account can hold. "user" is the family tier: full family-level
 * access with no booking history required, ever. Self-registration still
 * lands people as "guest"; only an admin promotes.
 */
export type Role = 'guest' | 'user' | 'admin';

export const ROLES: Role[] = ['guest', 'user', 'admin'];

/** How each role reads, for the admin panel. */
export const ROLE_LABELS: Record<Role, string> = {
  guest: 'Guest',
  user: 'Family',
  admin: 'Admin',
};

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
  /** When an admin blocked this account, or null if it is in good standing.
   * A blocked account keeps all of its history and simply stops being able to
   * sign in or act — the API re-checks this on every request, so a block lands
   * on a session that is already open. */
  blocked_at: string | null;
  avatar_url: string | null;
  /** Whether an admin password is set. Never the hash itself — the API only
   * ever reports its presence. Always false for guests. */
  has_password: boolean;
  last_login_at: string | null;
  created_at: string;
  updated_at: string;
}

/** What a booking delete took with it. Both children are unique per booking,
 *  so these are booleans rather than counts. */
export interface DeleteSummary {
  deleted: boolean;
  journal_entry: boolean;
  checkout: boolean;
}

/** The API's plain `{ message }` acknowledgement. */
export interface MessageResponse {
  message: string;
}

export interface UserWithStats extends User {
  booking_count: number;
  journal_count: number;
  /** Messages sent *or* received — either direction is history worth keeping. */
  message_count: number;
}

/**
 * A booking as the current caller may see it. Fields after `is_mine` are only
 * present for the booking's owner and for admins — the public calendar shows
 * that the camp is taken, never by whom.
 */
export type JournalStatus = 'pending' | 'approved' | 'rejected';

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
  /** Same visibility as guest_name etc. — the booking's owner and admins. */
  checked_out?: boolean;
  checkout_notes?: string;
  journal_id?: string;
  journal_status?: JournalStatus;
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
  rules: { id: string; text: string }[];
  amenities: { id: string; label: string; icon: string | null }[];
  gallery: { id: string; url: string; caption: string | null }[];
}

/**
 * The camp photo album — GET /api/guest-photos-link. Auth-gated, so it is a
 * separate fetch rather than part of the public `SiteContent` payload. Who
 * gets in is admin-configured (see {@link ContentAccessSection}): by default
 * the "user" family role, plus anyone who has ever had a booking approved.
 * `url` is null when no admin has set a link yet; anyone not admitted gets a
 * 403 instead.
 */
export interface GuestPhotosLink {
  url: string | null;
}

/**
 * One gated section and who may see it — GET /api/admin/content-access.
 *
 * `configurable: false` marks a section whose rule turns on a booking rather
 * than a role (writing a journal entry, check-in info). It is listed so the
 * admin sees the whole picture, but the server refuses edits to it.
 */
export interface ContentAccessSection {
  section_key: string;
  label: string;
  description: string;
  allowed_roles: Role[];
  approved_booking_grants: boolean;
  configurable: boolean;
  sort_order: number;
  updated_at: string;
}

/** Admin-only read of the editable settings — GET /api/admin/site-content/settings. */
export interface SiteSettingsAdmin {
  hero_title: string;
  hero_subtitle: string;
  about_text: string;
  hero_image_url: string | null;
  guest_photos_url: string | null;
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

// ── checkout checklist ──

export interface ChecklistItem {
  id: string;
  label: string;
  sort_order: number;
  active: boolean;
  created_at: string;
  updated_at: string;
}

export interface CheckoutEligibleBooking {
  id: string;
  check_in: string;
  check_out: string;
  guest_count_adults: number;
  guest_count_kids: number;
}

export interface CheckoutResponse {
  success: boolean;
  booking_id: string;
  journal_eligible: boolean;
}

export interface AdminChecklistState {
  id: string;
  label: string;
  checked: boolean;
}

export interface AdminCheckout {
  id: string;
  booking_id: string;
  guest_name: string;
  guest_email: string;
  check_in: string;
  check_out: string;
  completed_at: string;
  notes: string | null;
  items: AdminChecklistState[];
}

// ── camp journal ──

/** A journal entry as its own author sees it — any status. */
export interface JournalEntry {
  id: string;
  user_id: string;
  booking_id: string;
  title: string;
  body: string;
  status: JournalStatus;
  rejected_reason: string | null;
  approved_at: string | null;
  approved_by: string | null;
  archived_at: string | null;
  created_at: string;
  updated_at: string;
}

/** An approved entry as it appears on the public feed — no guest email, no ratings. */
export interface PublicJournalEntry {
  id: string;
  title: string;
  body: string;
  created_at: string;
  approved_at: string | null;
  guest_first_name: string;
  check_in: string;
  check_out: string;
}

export interface PublicJournalPage {
  entries: PublicJournalEntry[];
  page: number;
  page_size: number;
  total: number;
  total_pages: number;
}

export interface JournalEligibleBooking {
  booking_id: string;
  check_in: string;
  check_out: string;
}

export interface AdminJournalEntry {
  id: string;
  title: string;
  body: string;
  status: JournalStatus;
  rejected_reason: string | null;
  created_at: string;
  approved_at: string | null;
  approved_by: string | null;
  /** Set when an admin has hidden this entry from the public feed; independent of status. */
  archived_at: string | null;
  guest_name: string | null;
  guest_email: string;
  check_in: string;
  check_out: string;
}

// ── check-in info / my stay ──

/** Admin-only, full row (with sort_order) for the Check-In Info admin tab. */
export interface CheckinInfoItem {
  id: string;
  title: string;
  body: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

/** What a guest with active check-in access sees — no sort_order/timestamps. */
export interface CheckinInfoGuestItem {
  id: string;
  title: string;
  body: string;
}

/**
 * The guest's most relevant booking for the My Stay hub. `has_stay: false`
 * means no approved booking at all — every other field is then absent.
 */
export interface MyStay {
  has_stay: boolean;
  booking_id?: string;
  check_in?: string;
  check_out?: string;
  guest_count_adults?: number;
  guest_count_kids?: number;
  checked_out?: boolean;
  checkout_eligible?: boolean;
  /** Whether "/journal/new" should be offered — false once journal_status is set. */
  journal_eligible?: boolean;
  /** Set once an entry exists for this booking; show a status badge instead of the button. */
  journal_status?: JournalStatus;
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
  /** Barometric pressure, millibars. Absent on the forecast fallback. */
  pressure_mb?: number | null;
  /** Direction of the barometric change over the last ~6 h. */
  pressure_trend?: 'falling' | 'rising' | 'steady';
  /** Signed change in millibars over that window. */
  pressure_change_mb?: number | null;
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

/** One hourly point on the continuous tide curve. Same local-time format as {@link TidePrediction}. */
export interface TideCurvePoint {
  time: string;
  height_ft: number;
}

export interface Tides {
  station_id: string;
  station_name: string | null;
  timezone: string;
  /** The hi/lo turning points — the plain-text list. */
  next_tides: TidePrediction[];
  /** Hourly heights across ~48 h (last ~6 h → next ~42 h). Empty when NOAA's
   * continuous feed failed but hi/lo succeeded — render the list only. */
  curve: TideCurvePoint[];
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

/** A solunar bite window, local "HH:MM" (may cross midnight, so end < start). */
export interface SolunarPeriod {
  start: string;
  end: string;
}

export type TideStrength = 'strong' | 'average' | 'weak';

/** Per-factor breakdown of the star score. `pressure` is present for today
 * only — the forecast feed carries no barometric data for future days. */
export interface FishingFactors {
  /** 0–4, continuous: full weight on the day of new/full, 0 six days out. */
  moon: number;
  wind: number;
  tide: number;
  pressure?: number;
  /** The bounded `pressure + wind + tide` sum that actually reaches the score
   * (clamped to −2.5…+1.0 so weather shapes within the moon's envelope). */
  weather_adjustment: number;
}

export interface FishingDay {
  date: string;
  /** 1–5: `1 + moon(0–3) + pressure + wind + tide`, rounded and clamped. */
  stars: number;
  rating_label: 'Poor' | 'Fair' | 'Good' | 'Excellent';
  moon_phase: string;
  moon_emoji: string;
  /** Day's predicted tidal range vs the station's Great Diurnal Range. */
  tide_strength: TideStrength;
  /** Whether a weather term actually reached this day's score. True for today
   * (live observation) and the ~7 days the NWS forecast covers; false beyond
   * that, where the rating is moon and tide only. */
  weather_included: boolean;
  factors: FishingFactors;
  /** ~2 h, bracketing the moon's upper and lower transit. */
  major_periods: SolunarPeriod[];
  /** ~1 h, bracketing moonrise and moonset. */
  minor_periods: SolunarPeriod[];
}

export interface FishingForecast {
  location: string;
  timezone: string;
  generated_at: string;
  /** Present on an explicit `start`/`end` lookup; absent on the `days=N` form. */
  start?: string;
  end?: string;
  disclaimer: string;
  days: FishingDay[];
}

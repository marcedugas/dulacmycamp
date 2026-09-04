import { useQueries, useQuery } from '@tanstack/react-query';
import { api } from './api';
import type {
  AdminCheckout,
  AdminJournalEntry,
  AmenityItem,
  BlackoutDate,
  Booking,
  ChecklistItem,
  CheckoutEligibleBooking,
  GalleryPhoto,
  Holiday,
  JournalEligibleBooking,
  JournalEntry,
  Message,
  PublicJournalPage,
  RuleItem,
  SiteContent,
  SpecialEvent,
  UserWithStats,
} from './types';

export interface PublicConfig {
  camp_name: string;
  location: string;
  capacity_adults: number;
  latitude: number;
  longitude: number;
}

/** Camp settings, including the adult capacity the UI warns against. */
export function useConfig() {
  return useQuery({
    queryKey: ['config'],
    queryFn: () => api<PublicConfig>('/config', { anonymous: true }),
    staleTime: Infinity,
  });
}

/**
 * Bookings visible to the caller. Anonymous visitors get approved stays only;
 * the server decides — the client never filters for privacy.
 */
export function useBookings(params?: { mine?: boolean; status?: string }) {
  const search = new URLSearchParams();
  if (params?.mine) search.set('mine', 'true');
  if (params?.status) search.set('status', params.status);
  const qs = search.toString();

  return useQuery({
    queryKey: ['bookings', params?.mine ?? false, params?.status ?? ''],
    queryFn: () => api<Booking[]>(`/bookings${qs ? `?${qs}` : ''}`),
  });
}

export function useBlackouts() {
  return useQuery({
    queryKey: ['blackout-dates'],
    queryFn: () => api<BlackoutDate[]>('/blackout-dates', { anonymous: true }),
  });
}

export function useEvents() {
  return useQuery({
    queryKey: ['events'],
    queryFn: () => api<SpecialEvent[]>('/events', { anonymous: true }),
  });
}

/**
 * Reference-only US holiday markers for whichever year(s) the calendar has
 * on screen. One request per year, fetched in parallel and merged — each
 * year's result is a pure computation, so it's cached forever and never
 * refetched once seen.
 */
export function useHolidays(years: number[]) {
  const uniqueYears = Array.from(new Set(years)).sort((a, b) => a - b);

  const results = useQueries({
    queries: uniqueYears.map((year) => ({
      queryKey: ['holidays', year],
      queryFn: () => api<Holiday[]>(`/holidays?year=${year}`, { anonymous: true }),
      staleTime: Infinity,
    })),
  });

  return {
    data: results.flatMap((r) => r.data ?? []),
    isLoading: results.some((r) => r.isLoading),
  };
}

/** Hero/about text, rules, amenities, and gallery for the landing page. */
export function useSiteContent() {
  return useQuery({
    queryKey: ['site-content'],
    queryFn: () => api<SiteContent>('/site-content', { anonymous: true }),
  });
}

/** Admin-only: full rule rows (with sort_order) for the Site Content tab. */
export function useRulesAdmin() {
  return useQuery({
    queryKey: ['admin-rules'],
    queryFn: () => api<RuleItem[]>('/admin/rules'),
  });
}

/** Admin-only: full amenity rows (with sort_order) for the Site Content tab. */
export function useAmenitiesAdmin() {
  return useQuery({
    queryKey: ['admin-amenities'],
    queryFn: () => api<AmenityItem[]>('/admin/amenities'),
  });
}

/** Admin-only: full gallery rows (with sort_order) for the Site Content tab. */
export function useGalleryAdmin() {
  return useQuery({
    queryKey: ['admin-gallery'],
    queryFn: () => api<GalleryPhoto[]>('/admin/gallery'),
  });
}

// ── checkout checklist ──

/** Active checklist items, for rendering the checkout form. */
export function useChecklist() {
  return useQuery({
    queryKey: ['checklist'],
    queryFn: () => api<ChecklistItem[]>('/checklist'),
  });
}

/** Admin-only: every checklist item, including inactive ones. */
export function useChecklistAdmin() {
  return useQuery({
    queryKey: ['admin-checklist'],
    queryFn: () => api<ChecklistItem[]>('/admin/checklist'),
  });
}

/** The caller's own checkout-eligible bookings — usually zero or one. */
export function useCheckoutEligible() {
  return useQuery({
    queryKey: ['checkout-eligible'],
    queryFn: () => api<CheckoutEligibleBooking[]>('/checkout/eligible'),
  });
}

/** Admin-only: completed checkouts with per-item state and any notes. */
export function useAdminCheckouts() {
  return useQuery({
    queryKey: ['admin-checkouts'],
    queryFn: () => api<AdminCheckout[]>('/admin/checkouts'),
  });
}

// ── camp journal ──

/** Public, approved-only journal feed, paginated. */
export function useJournalPublic(page = 1) {
  return useQuery({
    queryKey: ['journal-public', page],
    queryFn: () => api<PublicJournalPage>(`/journal?page=${page}`, { anonymous: true }),
  });
}

/** The caller's own journal entries, any status. */
export function useJournalMine() {
  return useQuery({
    queryKey: ['journal-mine'],
    queryFn: () => api<JournalEntry[]>('/journal/mine'),
  });
}

/** Checked-out stays with no journal entry yet — drives /journal/new. */
export function useJournalEligibleBookings() {
  return useQuery({
    queryKey: ['journal-eligible-bookings'],
    queryFn: () => api<JournalEligibleBooking[]>('/journal/eligible-bookings'),
  });
}

/** Admin-only: every journal entry, optionally filtered by status. */
export function useJournalAdmin(status?: string) {
  return useQuery({
    queryKey: ['journal-admin', status ?? 'all'],
    queryFn: () => api<AdminJournalEntry[]>(`/journal/admin${status ? `?status=${status}` : ''}`),
  });
}

/** Pending-review count, for the admin panel's Journal tab badge. */
export function useJournalPendingCount(enabled = true) {
  const { data } = useQuery({
    queryKey: ['journal-admin', 'pending'],
    queryFn: () => api<AdminJournalEntry[]>('/journal/admin?status=pending'),
    enabled,
    refetchInterval: 120_000,
  });
  return data?.length ?? 0;
}

export function useMessages(all = false) {
  return useQuery({
    queryKey: ['messages', all],
    queryFn: () => api<Message[]>(`/messages${all ? '?all=true' : ''}`),
  });
}

/** Admin-only roster. Pass `enabled: false` for guests — `/users` 403s. */
export function useUsers(enabled = true) {
  return useQuery({
    queryKey: ['users'],
    queryFn: () => api<UserWithStats[]>('/users'),
    enabled,
  });
}

/** Everything the calendar needs, in one hook. */
export function useCalendarData() {
  const bookings = useBookings();
  const blackouts = useBlackouts();
  const events = useEvents();
  const config = useConfig();

  return {
    bookings: bookings.data ?? [],
    blackouts: blackouts.data ?? [],
    events: events.data ?? [],
    capacityLimit: config.data?.capacity_adults ?? 6,
    isLoading: bookings.isLoading || blackouts.isLoading || events.isLoading,
    isError: bookings.isError || blackouts.isError || events.isError,
  };
}

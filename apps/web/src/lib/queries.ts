import { useQueries, useQuery } from '@tanstack/react-query';
import { api } from './api';
import type { BlackoutDate, Booking, Holiday, Message, SpecialEvent, UserWithStats } from './types';

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

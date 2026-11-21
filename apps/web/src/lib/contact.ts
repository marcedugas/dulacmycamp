import type { WatercraftKind } from './types';

export const WATERCRAFT_OPTIONS: { value: WatercraftKind; label: string }[] = [
  { value: 'none', label: 'None' },
  { value: 'boat', label: 'Boat' },
  { value: 'kayak_canoe', label: 'Kayak or canoe' },
  { value: 'other', label: 'Other' },
];

export const WATERCRAFT_LABELS: Record<WatercraftKind, string> = {
  none: 'None',
  boat: 'Boat',
  kayak_canoe: 'Kayak or canoe',
  other: 'Other',
};

/**
 * Mirrors the server's phone rule (`contact::normalize_phone`), which stays
 * the authority: digits, spaces, dashes, dots and parentheses, an optional
 * leading +, 10 to 15 digits. Returns an error message, or null when fine.
 */
export function phoneError(raw: string): string | null {
  const t = raw.trim();
  if (!t) return null;
  if (!/^\+?[\d\s\-().]+$/.test(t)) {
    return 'Phone number can only contain digits, spaces, dashes and parentheses.';
  }
  const digits = t.replace(/\D/g, '').length;
  if (digits < 10 || digits > 15) return 'Phone number should have 10 to 15 digits.';
  return null;
}

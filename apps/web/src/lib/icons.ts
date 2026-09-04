import {
  Anchor,
  Bath,
  Bed,
  Camera,
  Car,
  CloudRain,
  Coffee,
  Compass,
  Dog,
  DoorOpen,
  Fish,
  Flame,
  Flashlight,
  Gamepad2,
  Key,
  Lock,
  MapPin,
  Microwave,
  Moon,
  Mountain,
  Music,
  ParkingCircle,
  PawPrint,
  Refrigerator,
  Sailboat,
  ShieldCheck,
  Ship,
  ShowerHead,
  Snowflake,
  Sparkles,
  Sun,
  Thermometer,
  Trees,
  TreePine,
  Tv,
  Umbrella,
  Utensils,
  WashingMachine,
  Waves,
  Wifi,
  Wind,
  type LucideIcon,
} from 'lucide-react';

/**
 * Curated lucide-react icons an amenity's free-text `icon` field may name —
 * the admin types a name like "Wifi" or "Anchor" (see AMENITIES in the old
 * hardcoded Landing.tsx for the original set this replaces). Anything not in
 * this list, including a blank field, falls back to a generic icon rather
 * than breaking the amenity card.
 */
const AMENITY_ICONS: Record<string, LucideIcon> = {
  Anchor,
  Bath,
  Bed,
  Camera,
  Car,
  CloudRain,
  Coffee,
  Compass,
  Dog,
  DoorOpen,
  Fish,
  Flame,
  Flashlight,
  Gamepad2,
  Key,
  Lock,
  MapPin,
  Microwave,
  Moon,
  Mountain,
  Music,
  ParkingCircle,
  PawPrint,
  Refrigerator,
  Sailboat,
  ShieldCheck,
  Ship,
  ShowerHead,
  Snowflake,
  Sun,
  Thermometer,
  Trees,
  TreePine,
  Tv,
  Umbrella,
  Utensils,
  WashingMachine,
  Waves,
  Wifi,
  Wind,
};

export const AMENITY_ICON_NAMES = Object.keys(AMENITY_ICONS).sort();

/** Case-insensitive lookup with a generic fallback for blank/unrecognized names. */
export function amenityIcon(name: string | null | undefined): LucideIcon {
  if (!name) return Sparkles;
  const key = Object.keys(AMENITY_ICONS).find((k) => k.toLowerCase() === name.trim().toLowerCase());
  return key ? AMENITY_ICONS[key] : Sparkles;
}

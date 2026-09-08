import { useQuery } from '@tanstack/react-query';
import { format } from 'date-fns';
import { Droplets, Fish, Moon, Star, Sunrise, Sunset, Waves, Wind } from 'lucide-react';
import { api } from '../lib/api';
import type {
  FishingForecast,
  Lunar,
  SolunarPeriod,
  TideCurvePoint,
  TidePrediction,
  Tides,
  Weather,
} from '../lib/types';
import { parseDay } from '../lib/dates';
import { Card, Spinner, cx } from './ui';

/**
 * "YYYY-MM-DD HH:MM" for *now* in the given IANA zone.
 *
 * NOAA returns tide times in the station's local time with no offset, so to
 * decide which are still upcoming we need the current wall clock down at the
 * station — not on the device, which may be anywhere.
 */
function nowAtZone(timeZone: string): string {
  const parts = new Intl.DateTimeFormat('en-CA', {
    timeZone,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  }).formatToParts(new Date());
  const get = (t: string) => parts.find((p) => p.type === t)?.value ?? '00';
  return `${get('year')}-${get('month')}-${get('day')} ${get('hour')}:${get('minute')}`;
}

/** Parses NOAA's local "YYYY-MM-DD HH:MM" for display formatting only. */
function parseStationTime(s: string): Date {
  return new Date(s.replace(' ', 'T'));
}

function WidgetShell({
  title,
  icon,
  loading,
  error,
  children,
}: {
  title: string;
  icon: React.ReactNode;
  loading: boolean;
  error: boolean;
  children: React.ReactNode;
}) {
  return (
    <Card className="flex flex-col">
      <div className="mb-3 flex items-center gap-2 text-sm font-bold uppercase tracking-wide text-muted">
        {icon}
        {title}
      </div>
      {loading ? (
        <div className="flex flex-1 items-center justify-center py-6">
          <Spinner />
        </div>
      ) : error ? (
        <p className="py-4 text-sm text-muted">
          Couldn&apos;t reach NOAA right now. Try again in a few minutes.
        </p>
      ) : (
        children
      )}
    </Card>
  );
}

// ─────────────────────────── weather ───────────────────────────

export function WeatherWidget() {
  const { data, isLoading, isError } = useQuery({
    queryKey: ['weather'],
    queryFn: () => api<Weather>('/weather', { anonymous: true }),
    staleTime: 15 * 60_000,
  });

  const c = data?.current;

  return (
    <WidgetShell
      title="Weather on the water"
      icon={<Wind size={15} />}
      loading={isLoading}
      error={isError || !data}
    >
      <p className="-mt-1 mb-3 text-xs text-muted">
        {data?.location ?? 'Cocodrie estuary'} — the water, not the camp
      </p>

      <div className="flex items-baseline gap-3">
        <span className="font-display text-5xl font-bold text-forest-600">
          {c?.temp_f != null ? Math.round(c.temp_f) : '—'}
          <span className="align-top text-2xl">°F</span>
        </span>
        <div className="text-sm">
          <p className="font-semibold text-charcoal">{c?.conditions ?? 'Conditions unavailable'}</p>
          <p className="text-muted">
            {c?.wind_mph != null
              ? `Wind ${Math.round(c.wind_mph)} mph`
              : (c?.wind_text ?? 'Wind —')}
            {c?.humidity != null && ` · ${Math.round(c.humidity)}% humidity`}
          </p>
          {c?.pressure_mb != null && (
            <p className="text-muted">
              {c.pressure_mb.toFixed(0)} mb
              {c.pressure_trend && c.pressure_trend !== 'steady' && (
                <span className="ml-1">{c.pressure_trend === 'falling' ? '↓ falling' : '↑ rising'}</span>
              )}
            </p>
          )}
        </div>
      </div>

      <div className="-mx-1 mt-5 flex gap-2 overflow-x-auto px-1 pb-1">
        {data?.forecast.map((d) => (
          <div
            key={d.start_time}
            title={d.detailed_forecast}
            className="min-w-[86px] flex-1 rounded-lg border border-sand bg-cream-dark/50 p-2 text-center"
          >
            <p className="truncate text-[11px] font-bold uppercase tracking-wide text-muted">
              {d.name.replace(' Night', ' nt')}
            </p>
            {d.icon && (
              <img src={d.icon} alt="" className="mx-auto my-1 h-9 w-9 rounded" loading="lazy" />
            )}
            <p className="text-sm font-bold text-charcoal">
              {d.high_f ?? '—'}°
              {d.low_f != null && <span className="font-normal text-muted"> / {d.low_f}°</span>}
            </p>
            <p className="truncate text-[10px] text-muted" title={d.short_forecast}>
              {d.short_forecast}
            </p>
            {d.precip_chance != null && d.precip_chance > 0 && (
              <p className="text-[10px] font-semibold text-bayou-600">{d.precip_chance}% rain</p>
            )}
          </div>
        ))}
      </div>
    </WidgetShell>
  );
}

// ─────────────────────────── tides ───────────────────────────

/** Parses the station-local "YYYY-MM-DD HH:MM" to an epoch for axis maths.
 * Every series here (curve, hi/lo marks, "now") goes through this, so the
 * graph is internally consistent regardless of the viewer's timezone. */
function tideEpoch(s: string): number {
  return new Date(s.replace(' ', 'T')).getTime();
}

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

/**
 * Hand-drawn SVG of the tide's rise and fall — an area + line through the
 * hourly points, the hi/lo turning points dotted and labelled, and a dashed
 * "now" marker. Vector, no charting library, palette-matched.
 */
function TideCurve({
  curve,
  marks,
  timezone,
}: {
  curve: TideCurvePoint[];
  marks: TidePrediction[];
  timezone: string;
}) {
  const W = 340;
  const H = 132;
  const pad = { top: 18, right: 8, bottom: 16, left: 8 };

  const pts = curve.map((p) => ({ t: tideEpoch(p.time), h: p.height_ft }));
  const minT = pts[0].t;
  const maxT = pts[pts.length - 1].t;
  const spanT = maxT - minT || 1;
  const hs = pts.map((p) => p.h);
  const minH = Math.min(...hs);
  const maxH = Math.max(...hs);
  const spanH = maxH - minH || 1;

  const x = (t: number) => pad.left + (clamp(t, minT, maxT) - minT) / spanT * (W - pad.left - pad.right);
  const y = (h: number) => pad.top + (1 - (h - minH) / spanH) * (H - pad.top - pad.bottom);

  const line = pts.map((p, i) => `${i ? 'L' : 'M'}${x(p.t).toFixed(1)},${y(p.h).toFixed(1)}`).join(' ');
  const base = H - pad.bottom;
  const area = `${line} L${x(maxT).toFixed(1)},${base} L${x(minT).toFixed(1)},${base} Z`;

  const nowT = tideEpoch(nowAtZone(timezone));
  const inFrame = marks.filter(
    (m) => m.height_ft != null && tideEpoch(m.time) >= minT && tideEpoch(m.time) <= maxT,
  );

  return (
    <svg
      viewBox={`0 0 ${W} ${H}`}
      className="w-full"
      role="img"
      aria-label="Tide height over the next two days"
    >
      <path d={area} fill="var(--color-bayou-100)" />
      <line x1={pad.left} y1={base} x2={W - pad.right} y2={base} stroke="var(--color-sand)" />
      <path
        d={line}
        fill="none"
        stroke="var(--color-bayou-500)"
        strokeWidth={1.75}
        strokeLinejoin="round"
        strokeLinecap="round"
      />

      {nowT >= minT && nowT <= maxT && (
        <g>
          <line
            x1={x(nowT)}
            y1={pad.top - 6}
            x2={x(nowT)}
            y2={base}
            stroke="var(--color-clay)"
            strokeWidth={1}
            strokeDasharray="3 2"
          />
          <text
            x={clamp(x(nowT), 12, W - 12)}
            y={pad.top - 9}
            textAnchor="middle"
            fontSize={8}
            fontWeight={700}
            fill="var(--color-clay)"
          >
            now
          </text>
        </g>
      )}

      {inFrame.map((m) => {
        const cx = x(tideEpoch(m.time));
        const cy = y(m.height_ft as number);
        const high = m.kind === 'high';
        return (
          <g key={m.time}>
            <circle cx={cx} cy={cy} r={2.4} fill="var(--color-forest-600)" />
            <text
              x={clamp(cx, 22, W - 22)}
              y={high ? cy - 6 : cy + 12}
              textAnchor="middle"
              fontSize={8.5}
              fontWeight={600}
              fill="var(--color-charcoal)"
            >
              {(m.height_ft as number).toFixed(1)} ft
              <tspan fill="var(--color-muted)" fontWeight={400}>
                {' '}
                {format(parseStationTime(m.time), 'h:mma').toLowerCase()}
              </tspan>
            </text>
          </g>
        );
      })}
    </svg>
  );
}

export function TideWidget({ count = 4 }: { count?: number }) {
  const { data, isLoading, isError } = useQuery({
    queryKey: ['tides'],
    queryFn: () => api<Tides>('/tides', { anonymous: true }),
    staleTime: 30 * 60_000,
  });

  const upcoming = (() => {
    if (!data) return [];
    const now = nowAtZone(data.timezone);
    return data.next_tides.filter((p) => p.time >= now).slice(0, count);
  })();

  return (
    <WidgetShell
      title="Tides"
      icon={<Waves size={15} />}
      loading={isLoading}
      error={isError || !data}
    >
      <p className="-mt-1 mb-3 text-xs text-muted">
        {data?.station_name ?? 'NOAA station'} · #{data?.station_id}
      </p>

      {data && data.curve.length >= 2 && (
        <div className="mb-4 rounded-lg border border-sand bg-cream-dark/40 p-2">
          <TideCurve curve={data.curve} marks={data.next_tides} timezone={data.timezone} />
        </div>
      )}

      {upcoming.length === 0 ? (
        <p className="text-sm text-muted">No upcoming predictions.</p>
      ) : (
        <ul className="space-y-2">
          {upcoming.map((t) => (
            <li key={t.time} className="flex items-center justify-between gap-3">
              <span
                className={cx(
                  'rounded-full px-2 py-0.5 text-[11px] font-bold uppercase tracking-wide',
                  t.kind === 'high'
                    ? 'bg-bayou-100 text-bayou-700'
                    : 'bg-wood-100 text-wood-700',
                )}
              >
                {t.kind}
              </span>
              <span className="flex-1 text-sm font-semibold text-charcoal">
                {format(parseStationTime(t.time), 'EEE h:mm a')}
              </span>
              <span className="text-sm tabular-nums text-muted">
                {t.height_ft != null ? `${t.height_ft.toFixed(1)} ft` : '—'}
              </span>
            </li>
          ))}
        </ul>
      )}
    </WidgetShell>
  );
}

// ─────────────────────────── moon + sun ───────────────────────────

export function LunarWidget() {
  const { data, isLoading, isError } = useQuery({
    queryKey: ['lunar'],
    queryFn: () => api<Lunar>('/lunar', { anonymous: true }),
    staleTime: 60 * 60_000,
  });

  const today = data?.days[0];

  return (
    <WidgetShell
      title="Moon & sun"
      icon={<Moon size={15} />}
      loading={isLoading}
      error={isError || !data}
    >
      <div className="flex items-center gap-4">
        <span className="text-5xl leading-none" aria-hidden>
          {data?.current.emoji}
        </span>
        <div>
          <p className="font-display text-lg font-bold text-charcoal">{data?.current.phase}</p>
          <p className="text-sm text-muted">{data?.current.illumination}% illuminated</p>
        </div>
      </div>

      <dl className="mt-4 space-y-2 border-t border-sand pt-3 text-sm">
        <div className="flex items-center justify-between gap-2">
          <dt className="flex items-center gap-1.5 text-muted">
            <Sunrise size={14} /> Sunrise
          </dt>
          <dd className="font-semibold text-charcoal">{today?.sunrise_local ?? '—'}</dd>
        </div>
        <div className="flex items-center justify-between gap-2">
          <dt className="flex items-center gap-1.5 text-muted">
            <Sunset size={14} /> Sunset
          </dt>
          <dd className="font-semibold text-charcoal">{today?.sunset_local ?? '—'}</dd>
        </div>
        <div className="flex items-center justify-between gap-2">
          <dt className="flex items-center gap-1.5 text-muted">
            <Droplets size={14} /> Next full moon
          </dt>
          <dd className="font-semibold text-charcoal">
            {data && format(parseDay(data.next_full_moon.date), 'MMM d')}
            <span className="ml-1 font-normal text-muted">
              ({data?.next_full_moon.days_away}d)
            </span>
          </dd>
        </div>
      </dl>
    </WidgetShell>
  );
}

// ─────────────────────────── fishing forecast ───────────────────────────

/** "14:30" (local, from the API) → "2:30 PM". */
function fmtPeriodTime(hm: string): string {
  const [h, m] = hm.split(':').map(Number);
  const d = new Date();
  d.setHours(h, m, 0, 0);
  return format(d, 'h:mm a');
}

function StarRating({ stars }: { stars: number }) {
  return (
    <span className="flex gap-0.5" aria-label={`${stars} out of 5 stars`}>
      {[1, 2, 3, 4, 5].map((n) => (
        <Star
          key={n}
          size={18}
          className={n <= stars ? 'fill-current text-wood-500' : 'text-sand'}
        />
      ))}
    </span>
  );
}

function PeriodColumn({ label, periods }: { label: string; periods: SolunarPeriod[] }) {
  return (
    <div>
      <p className="text-[11px] font-bold uppercase tracking-wide text-muted">{label}</p>
      {periods.length === 0 ? (
        <p className="text-muted">—</p>
      ) : (
        <ul className="mt-1 space-y-0.5">
          {periods.map((p) => (
            <li key={p.start} className="font-semibold tabular-nums text-charcoal">
              {fmtPeriodTime(p.start)} – {fmtPeriodTime(p.end)}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function FishingWidget() {
  const { data, isLoading, isError } = useQuery({
    queryKey: ['fishing-forecast'],
    queryFn: () => api<FishingForecast>('/fishing-forecast?days=7', { anonymous: true }),
    staleTime: 60 * 60_000,
  });

  const today = data?.days[0];

  return (
    <WidgetShell
      title="Fishing forecast"
      icon={<Fish size={15} />}
      loading={isLoading}
      error={isError || !data || !today}
    >
      {today && (
        <>
          <div className="flex items-center gap-3">
            <StarRating stars={today.stars} />
            <span className="font-display text-lg font-bold text-charcoal">
              {today.rating_label}
            </span>
            <span className="ml-auto text-2xl leading-none" aria-hidden>
              {today.moon_emoji}
            </span>
          </div>

          <p className="mt-2 text-xs text-muted">
            {today.moon_phase} · {today.tide_strength} tide
          </p>

          <div className="mt-4 grid grid-cols-2 gap-3 border-t border-sand pt-3 text-sm">
            <PeriodColumn label="Major periods" periods={today.major_periods} />
            <PeriodColumn label="Minor periods" periods={today.minor_periods} />
          </div>

          <div className="-mx-1 mt-4 flex gap-2 overflow-x-auto px-1 pb-1">
            {data?.days.map((d) => (
              <div
                key={d.date}
                title={`${d.rating_label} · ${d.moon_phase} · ${d.tide_strength} tide`}
                className="min-w-[64px] flex-1 rounded-lg border border-sand bg-cream-dark/50 p-2 text-center"
              >
                <p className="text-[11px] font-bold uppercase tracking-wide text-muted">
                  {format(parseDay(d.date), 'EEE')}
                </p>
                <p className="text-[10px] text-muted">{format(parseDay(d.date), 'M/d')}</p>
                <p className="mt-1 text-sm font-bold tracking-tight text-wood-600" aria-hidden>
                  {'★'.repeat(d.stars)}
                  <span className="text-sand">{'★'.repeat(5 - d.stars)}</span>
                </p>
              </div>
            ))}
          </div>

          <p className="mt-3 text-xs text-muted">
            {data?.disclaimer ?? 'Based on solunar theory — a fun guide, not a guarantee!'}
          </p>
        </>
      )}
    </WidgetShell>
  );
}

# Dulac My Camp

Booking and calendar app for a private fishing camp in Dulac, Louisiana.

Family and friends request dates, the owner approves or denies them with one
click straight from an email, and everyone can see what the camp — and the
weather, tides and moon — looks like before they drive down.

```
dulacmycamp/
├── apps/web/        React 19 · Vite · TypeScript · Tailwind v4
├── services/api/    Rust · Axum · SQLx · Postgres
└── .github/workflows/ci.yml
```

## Quick start

```bash
# 1. Postgres
docker run -d --name dulac-pg \
  -e POSTGRES_USER=dulac -e POSTGRES_PASSWORD=dulac -e POSTGRES_DB=dulacmycamp \
  -p 5432:5432 postgres:16-alpine

# 2. Config
cp .env.example services/api/.env     # edit DATABASE_URL + JWT_SECRET

# 3. API — migrations run automatically at boot
cd services/api && cargo run          # http://localhost:8080

# 4. Web (separate shell)
pnpm install && pnpm dev              # http://localhost:5173
```

With `RESEND_API_KEY` unset the API **logs** emails instead of sending them, so
you can read your own login code out of the server output. Or pull it straight
from the database:

```bash
docker exec dulac-pg psql -U dulac -d dulacmycamp -tAc \
  "SELECT code FROM otp_codes WHERE used=false ORDER BY created_at DESC LIMIT 1"
```

The seeded admin is `marc@recoresystems.net`. Any other address self-registers
as a guest the first time it asks for a login code.

## Checks

```bash
pnpm api:fmt && pnpm api:clippy && pnpm api:test   # Rust
pnpm typecheck && pnpm build                       # Web
```

## How it works

**Auth.** Email OTP in, JWT out. A six-digit code lives 10 minutes; the session
token lives 24 hours. There is no invite list — the owner's approve/deny step is
the real gate, so requiring an invitation to *ask* just adds friction. Every
request re-reads the user from the database rather than trusting the token's
claims, so a role change or a deleted account takes effect immediately.

**One-click approval.** Submitting a booking mints a 256-bit random token,
stored single-use on the row and valid 48 hours. The owner's email carries
`Approve` and `Deny` buttons pointing at `API_BASE_URL`; hitting either returns
a standalone HTML page (they're in a mail client, not the app) and clears the
token. `GET /api/bookings/deny/{token}` with no `?reason=` shows a small form;
with one, it's true one-click. The admin's copy of the email has no buttons —
admins act in the app.

**Overlaps are allowed on purpose.** Two cousins wanting the same weekend is a
conversation, not an error. Overlapping requests are accepted, flagged to the
guest on submit, and highlighted yellow in the admin table. Only *blackout
dates* hard-block a booking (409). Capacity works the same way: over
`CAPACITY_ADULTS`, the request is flagged, never refused.

**Privacy.** The calendar is public but anonymous — approved stays render as
"Booked" with a head count and no name. Names, emails, pets, requests and
checkout notes unlock for exactly three viewers: the guest who made the booking,
any admin, and anyone flagged `is_owner` (`User::sees_guest_details`). The
projection happens server-side in `BookingRow::to_view`; the client never
filters for privacy, so a guest's browser is never sent another guest's name.

**Nights, not days.** A stay occupies check-in through the night *before*
check-out, so the departure day shows free for the next guest.

## Environment

See [`.env.example`](.env.example) for the annotated list. The ones that matter:

| Variable | Required | Notes |
|---|---|---|
| `DATABASE_URL` | **yes** | Postgres connection string |
| `JWT_SECRET` | **yes** | `openssl rand -hex 32`; rotating it signs everyone out |
| `RESEND_API_KEY` | prod | Unset ⇒ emails are logged, not sent |
| `EMAIL_FROM_ADDRESS` | prod | Must be a Resend-verified domain |
| `FRONTEND_URL` | prod | CORS origin + links inside emails |
| `API_BASE_URL` | **prod** | Where approve/deny buttons point — must be reachable from the owner's inbox |
| `OWNER_EMAIL` | no | Bootstrap fallback only. Approval mail goes to every user flagged `is_owner` (admin panel → Users); this is used only when none is |
| `ADMIN_EMAIL` | no | Informational copy |
| `NOAA_STATION_ID` | no | Default `8762928` (Cocodrie) |
| `CAPACITY_ADULTS` | no | Default `10` |
| `VITE_API_URL` | prod | Build-time for the web app; changing it needs a redeploy |

## Deploying to Railway

Two services off this one repo:

**`dulacmycamp-api`** — root directory `services/api`, Dockerfile builder
(config in `services/api/railway.toml`). Attach a Postgres plugin; `DATABASE_URL`
is injected. Migrations run at boot. Health check is `/api/health`.

**`dulacmycamp-web`** — root directory `apps/web`, Nixpacks
(`apps/web/railway.toml`). Set `VITE_API_URL` to the API's public URL *before*
building — Vite inlines it.

Then close the loop: set the API's `API_BASE_URL` to its own public URL and
`FRONTEND_URL` to the web service's, and redeploy.

## External data

| Feed | Source | Cache |
|---|---|---|
| Weather | `api.weather.gov` — `/points/{lat},{lon}` → forecast grid + nearest station's recent observations | 30 min |
| Tides | NOAA CO-OPS `datagetter` — `interval=hilo` (7-day turning points) + `interval=h` (hourly curve, last ~6 h → next ~42 h) at station `8762928` | 1 h |
| Moon & sun | Computed locally — no upstream | n/a |
| Fishing forecast | Computed locally (solunar); star rating shaped by weather + tide strength | 1 h |
| Tide datums | CO-OPS metadata API — station Great Diurnal Range, for the tide-strength modifier | 24 h |

`api.weather.gov` rejects requests without a `User-Agent`; ours identifies the
app. Both proxied feeds are cached in-process because they change far more
slowly than the landing page is loaded. The observation call pulls a short
window (`?limit=12`), not just the latest reading, so the fishing forecast can
read a barometric *trend* off it; the window is cached once and shared.

**Coordinates.** The weather and fishing feeds use `FISHING_LAT`/`FISHING_LON`
(≈ 29.245, −90.662) — the Cocodrie estuary, matching the tide station, ~15 miles
south of the camp. That's the water people fish, and wind and pressure there
differ meaningfully from inland Dulac. `CAMP_LAT`/`CAMP_LON` (the camp itself)
still drive the "at the camp" sunrise/sunset, where 15 miles changes nothing.

Moon phase uses Meeus' *Astronomical Algorithms* ch. 49: the mean phase time
(JDE 2451550.09766 + 29.530588861·k) plus the periodic correction series. Mean
motion alone strays up to ~±14 h from the true syzygy over a year (the annual
term); the corrections pull it to under 4 minutes, so the phase *label* lands on
the right calendar day year-round. The fishing rating's moon term keys on the
same true distance-to-syzygy. Sunrise and sunset use the standard NOAA sunrise
equation.

**Fishing star rating.** `1 + moon(0–3) + pressure + wind + tide`, rounded to a
whole star, clamped 1–5. The moon term is a continuous gradient — 3 on the day
of new/full, tapering linearly to 0 at the quarters — so the day *of* a syzygy
outscores the days flanking it (the old flat buckets defaulted ~27 of every
29.5 days to 4–5 and left weather unable to move the needle). Pressure
(falling-sharp +1.5 / falling +0.5 / rising −1, today only) and wind
(calm +0.5 / >15 mph −1.5 / >25 mph −2.5) and **tide strength** all feed the
same sum. Tide strength is the day's predicted range (highest high − lowest
low) against the station's Great Diurnal Range: >1.15× → strong (+1),
<0.75× → weak (−1). Unlike pressure, tide predictions are good weeks out, so
this modifier applies to all seven days. A per-day `factors` object in the
response breaks the score down (`pressure` present for today only).

The fishing forecast needs the moon's actual *position*, not just its phase, so
it carries a truncated form of Jean Meeus' lunar series (*Astronomical
Algorithms*, ch. 47) and derives moonrise/set and upper/lower transit from it —
the major and minor solunar windows. All are unit-tested; the moon events are
checked against USNO rise/set/transit tables for four dates across 2026
(agreeing to ~1 minute) and cross-checked against a published solunar table for
Cocodrie, and the star rating's peak bracket is pinned to the Sept 2026 new-moon
transition.

## Deviations from the spec

1. **Tide station changed to `8762928`.** The spec said `8762075 — Cocodrie/Dulac
   area`, but NOAA reports `8762075` as **Port Fourchon, Belle Pass** (29.11,
   -90.20) — about 50 miles east. `8762928` is the actual **Cocodrie** gauge
   (29.245, -90.662), 15 miles south of the camp. The label was followed over
   the number. Override with `NOAA_STATION_ID`; the widget always shows the
   station's real name, so a wrong station is visible rather than silent.

2. **Weather feed pointed at the estuary, not the camp.** It was using the
   camp's own coordinates (29.3802, −90.7148); it now uses the Cocodrie
   estuary's (`FISHING_LAT`/`FISHING_LON`), matching the tide station. Same
   reasoning as deviation 1 — the label (water people fish) over the letter
   (the camp's dot on the map). Sun/moon times still use the camp.

3. **No `APPROVE_TOKEN_SECRET`.** Approve/deny tokens are 256-bit random values
   stored single-use in the database, which is strictly stronger than a signed
   token: it is revocable, can't be replayed, and there is no secret to leak or
   rotate. The variable is therefore absent rather than unused-but-present.

4. **PDF export is drawn, not screenshotted.** Tailwind v4 emits `oklch()`
   colours, which html2canvas cannot parse — it would have thrown at runtime.
   `src/lib/pdf.ts` draws the grid with jsPDF primitives instead: sharper,
   ~10× smaller, and vector. The Year view exports as 12 pages.

5. **Migrations live at `services/api/migrations/`** (SQLx's default), not
   `src/migrations/` as the file tree in the spec showed.

6. **Added `GET /api/config`** so the adult capacity isn't hardcoded twice and
   allowed to drift between API and UI.

7. **`cargo new` produced a single crate**, not a workspace — the API is small
   enough that splitting it would be ceremony.

8. **Solunar star rating is a continuous moon gradient, not the phase-bucket
   table.** The addendum's baseline table (new/full 5, quarter 3, shoulder 4) defaulted
   ~27 of every ~29.5 days to 4–5 before any weather modifier, and modifiers
   only subtracted — the effective range was ~3–5, not 1–5. The rating is now a
   continuous gradient (`1 + moon(0–3)`, moon peaking on the day of the syzygy)
   plus wider weather/tide modifiers, so the full 1–5 range is used. `phase_name`
   is unchanged — the lunar widget still wants one crisp label.

9. **Solunar star rating: barometric nudge is today-only.** The pressure trend
   is a *now* signal read from live observations; the forecast feed carries no
   pressure, so days 2–7 are scored on the moon, wind and tide alone. Also,
   a calendar day sometimes shows one major window rather than two — the second
   transit has simply crossed local midnight into the next day, where it's
   listed. Minor windows are ~1 h per the spec (some hobby tables use 2 h).

10. **Whole stars, not half-stars.** The rebalance's continuous score is rounded
    to a whole star for display. `stars` stays an integer 1–5 and the frontend
    is unchanged; the raw contributions are in the response's per-day `factors`
    for anyone who wants them. Half-star precision would overstate what a
    "fun guide" solunar score can claim.

11. **Tide strength is real predicted flow, not a phase proxy.** The addendum
    assumed strong tides fall on the new/full moon ("spring tides… will double
    up with the moon bonus"). That holds on Atlantic coasts; the northern Gulf
    is different. At Cocodrie the daily range tracks the **Moon's declination**
    (a ~27-day cycle that regresses, unrelated to phase), so the biggest ranges
    often land at the quarters and the *smallest* at new/full — near a node the
    new moon's range drops to ~0.3 ft against a 1.05 ft baseline. The modifier
    keys on the station's own predicted range, so it reflects actual water
    movement: some weeks it doubles the moon bonus, some weeks (like the
    2026-09-11 new moon) it works against it. That is the honest signal, not a
    bug — a dead 0.3 ft tide really does fish worse than a moving 1.4 ft one.

12. **Datums from the CO-OPS metadata API, not the `datagetter` datums
    product** — NOAA retired the latter. `baseline_tidal_range` reads Great
    Diurnal Range (GT) from `.../mdapi/prod/webapi/stations/{id}/datums.json`,
    cached a day; `None` on any failure drops the tide modifier to neutral.

13. **Tide `curve` timestamps are the station's local wall-clock string**
    (`"YYYY-MM-DD HH:MM"`), not the UTC `…Z` the fishing-forecast spec sketched.
    The hi/lo `next_tides` list has always used that format and stays unchanged;
    keeping the curve on the same string lets the widget put both series — and
    the "now" marker — on one axis with zero timezone arithmetic, and it renders
    the same clock for a viewer in any timezone.

## Before go-live

- [x] **Jean's email address** — `jldugas@eatel.net`, flagged `is_owner` by
      migration `0004`. The old typo `jeanldugas@eatel.net` is gone everywhere:
      `0004` folded any account under it into the real row, dropped its
      outstanding login codes, and Railway's `OWNER_EMAIL` was corrected
      2026-09-04. It only survives in that migration's comments, which describe
      history and should stay. `OWNER_EMAIL` can now be cleared outright — an
      owner is flagged, so the fallback is never read.
- [x] **Verify a sending domain in Resend** — `recoresystems.net`, confirmed
      2026-09-04 by a live send. `EMAIL_FROM_ADDRESS` is `camp@recoresystems.net`.
- [x] **Real photos** — hero and gallery are live, edited from admin →
      Site Content (migration `0005`), not hardcoded in `Landing.tsx`.
- [x] **Real house rules and amenities** — same place; the `RULES` and
      `AMENITIES` arrays are gone.
- [x] **Confirm capacity** — 10 adults. `CAPACITY_ADULTS` defaults to `10`;
      over it, a booking is flagged, never refused.
- [x] **Guests may not see each other's names.** Identities — name, email, pets,
      requests, checkout notes — go to the guest who booked, to admins, and to
      anyone flagged `is_owner`. Everyone else sees an anonymous "Booked" cell
      with a head count. Enforced server-side in `BookingRow::to_view` via
      `User::sees_guest_details`; the client never filters for privacy.

## Rate limiting

`POST /auth/request-otp` is the only public, unauthenticated endpoint that
spends money — every accepted call sends an email. It is throttled twice:

| Limit | Window | Why |
|---|---|---|
| 1 per email address | 60s | Stops one inbox being flooded with codes |
| 5 per client IP | 10 min | Caps spend; an attacker cycling addresses walks past a per-email limit |

Over the limit returns `429` with a message the login form shows as-is.
Code *entry* (`/auth/verify-otp`) is deliberately not throttled — that would
let anyone lock a guest out of their own account.

The counters are in-process (`src/rate_limit.rs`), which is a deliberate
trade-off: no Redis to operate, but limits reset on deploy and are per-replica.
Fine as a cost guard on a single-replica service; it would not be enough for
anything security-critical.

## Not built

Any automated test above the unit level on the API — no integration suite
spins up Postgres and exercises the HTTP paths. Worth adding.

---

© 2026 Dulac My Camp

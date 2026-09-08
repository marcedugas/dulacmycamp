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
| Weather | `api.weather.gov` — `/points/{lat},{lon}` → forecast grid + nearest station's latest observation | 30 min |
| Tides | NOAA CO-OPS `datagetter`, `interval=hilo`, 7 days at station `8762928` | 1 h |
| Moon & sun | Computed locally — no upstream | n/a |

`api.weather.gov` rejects requests without a `User-Agent`; ours identifies the
app. Both feeds are cached in-process because they change far more slowly than
the landing page is loaded.

Moon phase comes from the mean synodic month (29.530589 d) against a known new
moon epoch. Sunrise and sunset use the standard NOAA sunrise equation — accurate
to well under a minute at this latitude, which is all a widget needs. Both are
covered by unit tests, including a cross-check against the published September
2026 full moon.

## Deviations from the spec

1. **Tide station changed to `8762928`.** The spec said `8762075 — Cocodrie/Dulac
   area`, but NOAA reports `8762075` as **Port Fourchon, Belle Pass** (29.11,
   -90.20) — about 50 miles east. `8762928` is the actual **Cocodrie** gauge
   (29.245, -90.662), 15 miles south of the camp. The label was followed over
   the number. Override with `NOAA_STATION_ID`; the widget always shows the
   station's real name, so a wrong station is visible rather than silent.

2. **No `APPROVE_TOKEN_SECRET`.** Approve/deny tokens are 256-bit random values
   stored single-use in the database, which is strictly stronger than a signed
   token: it is revocable, can't be replayed, and there is no secret to leak or
   rotate. The variable is therefore absent rather than unused-but-present.

3. **PDF export is drawn, not screenshotted.** Tailwind v4 emits `oklch()`
   colours, which html2canvas cannot parse — it would have thrown at runtime.
   `src/lib/pdf.ts` draws the grid with jsPDF primitives instead: sharper,
   ~10× smaller, and vector. The Year view exports as 12 pages.

4. **Migrations live at `services/api/migrations/`** (SQLx's default), not
   `src/migrations/` as the file tree in the spec showed.

5. **Added `GET /api/config`** so the adult capacity isn't hardcoded twice and
   allowed to drift between API and UI.

6. **`cargo new` produced a single crate**, not a workspace — the API is small
   enough that splitting it would be ceremony.

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

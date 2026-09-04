# Deployment — Railway

Live as of 2026-09-04.

| | URL |
|---|---|
| Web | https://dulacmycamp-web-production.up.railway.app |
| API | https://dulacmycamp-api-production.up.railway.app |
| Project | https://railway.com/project/089814ba-313b-4eda-a546-f9c58566a190 |

Three services in the `production` environment: `dulacmycamp-api`,
`dulacmycamp-web`, and a `Postgres` plugin. Both app services deploy from
`marcedugas/dulacmycamp` on `main` and redeploy on push.

## Why there is no railway.toml

Railway **deprecated Config-as-Code** (`railway.json` / `railway.toml`) in
favour of Infrastructure-as-Code (`.railway/railway.ts`). The API rejects
writes that reference a config file:

```
Config as Code (railway.json / railway.toml) is deprecated.
Use Infrastructure as Code (.railway/railway.ts) instead.
```

The IaC authoring file needs Node 22+ (the CLI evaluates it with
`--experimental-strip-types`); this machine has Node 20, so the two
`railway.toml` files were removed and their settings applied directly to the
service instances instead. They are recorded below so nothing is lost.

**Migrating to IaC later** — on a machine with Node 22+:

```bash
railway config init          # scaffolds .railway/railway.ts
railway config pull          # imports the live project into it
railway config plan          # preview
railway config apply
```

That is the better end state: configuration back in the repo and reviewable.

## Current service settings

Set with `railway api` (`serviceInstanceUpdate`), not from a file.

**dulacmycamp-api**
| Setting | Value |
|---|---|
| Root directory | `services/api` |
| Builder | Railpack, which auto-detects the Dockerfile |
| Dockerfile path | `Dockerfile` (relative to the root directory) |
| Start command | `dulacmycamp-api` |
| Healthcheck | `/api/health`, 120s timeout |
| Restart policy | `ON_FAILURE`, max 5 |
| Watch patterns | `services/api/**` |
| Domain | port **8080** |

**dulacmycamp-web**
| Setting | Value |
|---|---|
| Root directory | `/` (repo root) |
| Builder | Nixpacks |
| Build command | `pnpm --filter @dulacmycamp/web build` |
| Watch patterns | `apps/web/**`, `pnpm-lock.yaml`, `package.json` |
| Domain | port **8080** |

Two things worth knowing about the web service:

1. **It builds from the repo root, not `apps/web`.** `pnpm-lock.yaml` lives at
   the workspace root, and Nixpacks runs its own `pnpm i --frozen-lockfile`
   before any build command. Rooted at `apps/web` that fails with
   `ERR_PNPM_NO_LOCKFILE`. Building from the root uses the committed lockfile,
   so deploys are reproducible.
2. **Nixpacks serves the build with Caddy on port 8080**, having detected a
   static site — it supersedes the `serve` start command, and handles SPA
   fallback (deep links like `/calendar` return the app, not a 404). The
   generated domain must therefore target **8080**, not 3000. A domain pointed
   at the wrong port fails as a 502 with a perfectly healthy container.

## Variables

Set on **dulacmycamp-api**:

| Variable | Value |
|---|---|
| `DATABASE_URL` | `${{Postgres.DATABASE_URL}}` |
| `JWT_SECRET` | 32 random bytes, generated at setup |
| `API_BASE_URL` | the API's own public URL |
| `FRONTEND_URL` | the web service's public URL |
| `OWNER_EMAIL` | `laihafloyd@gmail.com` — **temporary**, see below |
| `ADMIN_EMAIL` | `marc@recoresystems.net` |
| `CAPACITY_ADULTS` | `6` |
| `NOAA_STATION_ID` | `8762928` (Cocodrie) |
| `LOG_LEVEL` | `info,dulacmycamp_api=debug` |

Set on **dulacmycamp-web**: `VITE_API_URL` — Vite inlines it at build time, so
changing it needs a **redeploy**, not a restart.

`API_BASE_URL` is the one that fails silently: it builds the approve/deny links
in the owner's email. Point it somewhere unreachable and the buttons in real
emails are dead, with nothing failing anywhere else.

## Still to do before real guests use it

- [ ] **`OWNER_EMAIL` → `jeanldugas@eatel.net`.** Currently
      `laihafloyd@gmail.com` as a stand-in.
      `railway variables --service dulacmycamp-api --set OWNER_EMAIL=jeanldugas@eatel.net`
- [ ] **`RESEND_API_KEY` + `EMAIL_FROM_ADDRESS`.** Both unset, so **no email is
      being sent at all** — the API logs the message instead. Login codes are
      readable with `railway logs --service dulacmycamp-api`, which is fine for
      testing and useless for guests. Needs a verified sending domain in Resend.
- [ ] Rate-limit `/auth/request-otp`. It is public, unauthenticated, and sends
      mail — the obvious thing to abuse once mail is switched on.
- [ ] Replace placeholder photos, house rules and amenities in
      `apps/web/src/routes/Landing.tsx` (marked `TODO(content)`).
- [ ] Consider a custom domain.

## Operations

```bash
railway logs --service dulacmycamp-api            # runtime
railway logs --service dulacmycamp-api --build    # build
railway variables --service dulacmycamp-api       # inspect
railway redeploy --service dulacmycamp-web        # redeploy without a push
railway connect Postgres                          # psql shell
```

The database has no public proxy — it is reachable only from inside the
project's private network. Use `railway connect Postgres` rather than exposing
a TCP proxy.

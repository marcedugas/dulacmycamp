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
| Builder | Railpack |
| Build command | `pnpm --filter @dulacmycamp/web build` |
| Start command | `pnpm --filter @dulacmycamp/web exec serve -s dist -l $PORT` |
| Watch patterns | `apps/web/**`, `pnpm-lock.yaml`, `package.json`, `.node-version` |
| Domain | port **8080** (`PORT=8080` is set to match) |

Three things about the web service that cost a deploy each to discover:

1. **It builds from the repo root, not `apps/web`.** `pnpm-lock.yaml` lives at
   the workspace root, and the builder runs `pnpm i --frozen-lockfile` before
   any build command. Rooted at `apps/web` that fails with
   `ERR_PNPM_NO_LOCKFILE`. From the root the committed lockfile is used, so
   deploys are reproducible rather than re-resolving every build.

2. **The builder defaults to Node 18.** Vite 8 bundles rolldown, which imports
   `styleText` from `node:util` (Node 20.12+), so the build died with a
   `SyntaxError` before emitting anything. Fixed in the repo with
   `engines.node` and `.node-version` rather than a Railway-only setting, so
   the next environment doesn't hit the same wall.

3. **`serve` defaults to port 3000, the generated domain targeted 3000, and
   the container listened on 8080.** A port mismatch surfaces as a 502 with a
   perfectly healthy container and nothing useful in the logs. `PORT=8080` and
   the domain's target port are now pinned to each other.

A note on reading build failures: `railway logs --service <svc> --build`
returns the last *successful* build, which is deeply misleading when you are
chasing a failure. For the failed one, query it directly:

```bash
DEP=$(railway deployment list --service dulacmycamp-web --json | python3 -c "import json,sys;print(json.load(sys.stdin)[0]['id'])")
railway api 'query($id:String!){buildLogs(deploymentId:$id,limit:400){message}}' --raw-var "id=$DEP"
```

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
- [ ] **Remove the test account** `prod-check@example.com` — created while
      verifying the live OTP endpoint from a browser. It is an empty guest row
      with no bookings. There is no user-delete endpoint in the API, and
      `railway connect Postgres` needs an SSH key registered on the account
      (`railway ssh keys add`), so it was left in place rather than changing
      account security settings. Harmless, but it shows in the admin Users tab.

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

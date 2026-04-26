# Circle

A tiny, private, self-hosted-style social app for you and a few friends. Invite-only, no algorithm, no feed ranking, no discoverability. One post list, reverse-chronological. A "🔔" button for push notifications so you don't have to open the app to check.

Everything runs on Cloudflare's free tier: Workers for the API, D1 for data, R2 for photos, Pages for the client. All Rust, all WASM.

## Layout

- `shared/` – types used on both client and server
- `worker/` – Cloudflare Worker API (workers-rs)
- `client/` – Leptos CSR PWA (Trunk build)
- `schema.sql` – D1 migration

## Prereqs

```
rustup target add wasm32-unknown-unknown
cargo install worker-build trunk
npm i -g wrangler
wrangler login
```

## One-time Cloudflare setup

```
# D1
wrangler d1 create circle-db
# copy the database_id into wrangler.toml
wrangler d1 execute circle-db --file=schema.sql

# R2
wrangler r2 bucket create circle-media

# VAPID keys (P-256 private scalar, base64url)
# Generate with any web-push tool, e.g. Node:
#   npx web-push generate-vapid-keys
# Copy the output "privateKey" (it's already base64url).
wrangler secret put VAPID_PRIVATE_KEY   # paste privateKey
wrangler secret put VAPID_PUBLIC_KEY    # paste publicKey
wrangler secret put VAPID_SUBJECT       # mailto:you@example.com
```

## Create an invite code

Until you build an admin UI, seed invite codes by hand:

```
wrangler d1 execute circle-db --command \
  "INSERT INTO invite_codes (code, circle_id, created_at) \
   VALUES ('firstuser', 'default', strftime('%s','now'))"
```

## Dev loop

Two terminals:

```
# terminal 1 – API
cd worker
wrangler dev

# terminal 2 – client
cd client
trunk serve --open
```

The client defaults to `http://127.0.0.1:8787` for the API. Override with `API_BASE` when building for production:

```
cd client
API_BASE=https://circle-api.<your-subdomain>.workers.dev trunk build --release
```

## Deploy

Push to `main` and the included GitHub Action handles both worker and client. You need three repo-level secrets/vars:

- secret `CF_API_TOKEN` – Cloudflare API token with Workers + Pages + D1 + R2 scopes
- secret `CF_ACCOUNT_ID`
- var `API_BASE` – your deployed worker URL

For the no-custom-domain path, the client lives at `https://circle-app.pages.dev` (or whatever you name the project) and the worker at `https://circle-api.<sub>.workers.dev`. Set `ALLOWED_ORIGIN` in `wrangler.toml` to the Pages URL.

## iOS push

Web Push on iOS requires the PWA to be installed to the home screen (iOS 16.4+). After first load, Safari → Share → Add to Home Screen. Open the installed app, hit the 🔔 button to register.

## v1 → v1.1 punch list

- **Passkeys.** Replace the invite+token flow with `webauthn-rs` (rustcrypto feature). The `users` table already has room; just drop `sessions` in favor of JWTs tied to credential IDs.
- **Encrypted payloads.** Add `aes-gcm` + `hkdf` and implement RFC 8291 so push notifications carry the poster's name instead of a generic "someone posted" message.
- **Image upload UX.** Compress client-side to WebP (Canvas + `toBlob`, max width 1200px, quality 0.82), show a preview, PUT to `/api/media/<key>`, then submit the post with the returned `image_key`. Scaffold exists, the client-side encode step is the v1.1 task.
- **Comments UI.** Handlers are live; add the drawer in `components.rs`.
- **SSR.** If first-paint latency matters, revisit Leptos SSR on Workers once the `leptos_axum` + `worker::axum` story settles.

## Design notes

**Why no feed ranking?** The whole point. If nothing is new, the app says "you're caught up" and means it. No infinite scroll, no algorithmic ordering, no engagement metrics anywhere in the UI.

**Why tickle pushes instead of content pushes?** RFC 8291 payload encryption requires ECDH + HKDF + AES-GCM per subscriber. For v1 that's complexity we don't need: the notification wakes you, you tap it, the app fetches the new posts. Upgrade to encrypted payloads in v1.1 if the notification body itself needs to carry the poster's name.

**Why CSR instead of SSR?** Leptos SSR inside a Cloudflare Worker works but is finicky in early 2026 – the Tokio assumption in `leptos_axum` leaks in ways that cost a week to debug. CSR ships now, the WASM binary is ~300KB gzipped with wasm-opt, and the service worker caches it after first load. You can swap in SSR later without touching the API.

**Why self-rolled session tokens instead of JWTs?** Revocation. Deleting a row is simpler than maintaining a JWT denylist. At this scale, the DB hit is invisible.

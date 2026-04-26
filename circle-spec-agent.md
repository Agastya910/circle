# Circle App — Technical Spec (Agent Edition)

**Goal:** Private invite-only social PWA. Zero infra cost. Lightning-fast globally. Secure. Maintainable by one person.

---

## Stack

| Layer | Technology |
|---|---|
| Frontend | Leptos 0.7 (Rust → WASM, islands architecture) |
| Backend | `workers-rs` (Rust → WASM, Cloudflare Workers) |
| Database | Cloudflare D1 (serverless SQLite) |
| Media | Cloudflare R2 (S3-compatible, zero egress) |
| Push | Web Push VAPID (direct, no Firebase) |
| Auth | `webauthn-rs` (Passkeys / WebAuthn) |
| Proxy/TLS/WAF | Cloudflare Free Plan |
| CI/CD | GitHub Actions + `wrangler deploy` |
| Domain | Cloudflare Registrar (~$10/yr) |

---

## Compilation

```toml
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

Post-build: run `wasm-opt -Oz` on output WASM binary (Binaryen).

Target: `wasm32-unknown-unknown`.

---

## Frontend: Leptos 0.7

- Use `#[island]` for interactive components only. Static shell = pure HTML, no WASM shipped.
- Use `#[server]` server functions for all data fetching — no manual REST/fetch/JSON.
- Reactive state via Leptos signals — no VDOM, no re-renders, fine-grained DOM updates only.

```rust
#[island]
fn PostFeed() -> impl IntoView { ... }

#[server(GetPosts)]
pub async fn get_posts(cursor: Option<String>) -> Result<Vec<Post>, ServerFnError> {
    let db = use_context::<D1Database>()?;
    db.query("SELECT * FROM posts WHERE circle_id=?1 ORDER BY created_at DESC LIMIT 20")
      .bind(&[circle_id.into()])?.all().await?.results()
}
```

---

## Backend: Cloudflare Workers (`workers-rs`)

```rust
#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    Router::new()
        .get_async("/api/posts", get_posts)
        .post_async("/api/posts", create_post)
        .post_async("/api/reactions", add_reaction)
        .post_async("/api/auth/register", register_passkey)
        .post_async("/api/auth/login", authenticate_passkey)
        .post_async("/api/push/subscribe", subscribe_push)
        .run(req, env).await
}
```

**Free tier limits (per day):** 100K requests, 10ms CPU/request, 128MB memory.  
**Realistic usage at 10 users:** ~500 requests, ~2ms CPU. Headroom: 200×.

**Persistent state:** Use Cloudflare Durable Objects for any future realtime feature (e.g., live comments). Not needed for v1.

---

## Database: Cloudflare D1

Accessed via `env.d1("DB")` binding inside Workers. No network hop — same PoP as Worker.

```sql
CREATE TABLE users (
    id           TEXT PRIMARY KEY,
    circle_id    TEXT NOT NULL,
    display_name TEXT NOT NULL,
    avatar_key   TEXT,
    passkey_id   TEXT NOT NULL,
    public_key   BLOB NOT NULL,
    push_sub     TEXT,
    created_at   INTEGER NOT NULL
);

CREATE TABLE posts (
    id         TEXT PRIMARY KEY,
    circle_id  TEXT NOT NULL,
    author_id  TEXT NOT NULL REFERENCES users(id),
    body       TEXT,
    image_key  TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE reactions (
    id         TEXT PRIMARY KEY,
    post_id    TEXT NOT NULL REFERENCES posts(id),
    user_id    TEXT NOT NULL REFERENCES users(id),
    emoji      TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(post_id, user_id, emoji)
);

CREATE TABLE comments (
    id         TEXT PRIMARY KEY,
    post_id    TEXT NOT NULL REFERENCES posts(id),
    author_id  TEXT NOT NULL REFERENCES users(id),
    body       TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE invite_codes (
    code       TEXT PRIMARY KEY,
    used       INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_posts_circle_time ON posts(circle_id, created_at DESC);
CREATE INDEX idx_reactions_post ON reactions(post_id);
CREATE INDEX idx_comments_post ON comments(post_id);
```

**Rule:** Every query MUST filter by `circle_id`. No exceptions.

---

## Media: Cloudflare R2

**Upload flow:**
1. Client requests signed upload URL from Worker (authenticated).
2. Worker generates pre-signed R2 URL (60s TTL), scoped to one object key.
3. Client uploads directly to R2 — Worker not involved.
4. Client confirms key to Worker → Worker writes post record to D1.

**Read flow:**
1. Every image request goes through Worker.
2. Worker validates JWT + verifies post belongs to requester's circle.
3. Worker returns signed R2 read URL (5min TTL).

**Client-side before upload:** compress + convert to WebP via Canvas API (max width 1200px, quality 0.82). No server-side encoding needed.

**Free tier:** 10GB storage, 1M writes/mo, 10M reads/mo, zero egress fees.

---

## Auth: WebAuthn / Passkeys

**Crate:** `webauthn-rs`

**Registration:**
```
1. User presents invite code → Worker validates (single-use, D1).
2. Browser: navigator.credentials.create() → generates keypair on device.
3. Public key + attestation → Worker → webauthn-rs validates → stored in users.public_key.
4. Invite code marked used.
5. Issue JWT access token (15min) + refresh token (7-day, stored in D1).
```

**Session tokens:**
- Access: JWT, 15min TTL, signed with `CLOUDFLARE_SECRET` (Wrangler secret, never in source).
- Refresh: opaque UUID in D1, 7-day TTL, rotated on each use.
- Cookie flags: `HttpOnly; Secure; SameSite=Strict; Path=/api`.

---

## Push Notifications: Web Push VAPID

**No Firebase. No third-party service.**

```bash
# One-time setup
wrangler secret put VAPID_PRIVATE_KEY
wrangler secret put VAPID_PUBLIC_KEY
```

**Subscribe:** On PWA open, browser calls `pushManager.subscribe()` → returns `PushSubscription` JSON → POST to `/api/push/subscribe` → stored in `users.push_sub`.

**Fan-out on post:**
```rust
async fn notify_circle(env: &Env, circle_id: &str, post: &Post) -> Result<()> {
    let subs: Vec<PushSubscription> = db
        .prepare("SELECT push_sub FROM users WHERE circle_id=?1 AND id!=?2 AND push_sub IS NOT NULL")
        .bind(&[circle_id.into(), post.author_id.clone().into()])?
        .all().await?.results()?;
    for sub in subs {
        send_web_push(&env, &sub, &payload).await?;
    }
    Ok(())
}
```

**iOS:** Push notifications require PWA installed to home screen (iOS 16.4+). Onboarding must prompt install.

---

## Security Headers (apply to every response)

```rust
headers.set("Content-Security-Policy",
    "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; img-src 'self' blob: data:; connect-src 'self'")?;
headers.set("X-Content-Type-Options", "nosniff")?;
headers.set("X-Frame-Options", "DENY")?;
headers.set("Referrer-Policy", "strict-origin-when-cross-origin")?;
headers.set("Permissions-Policy", "camera=(self), microphone=(), geolocation=()")?;
```

**Rate limiting in Worker middleware:** 10 posts/hour per user, 60 API calls/minute per user.  
**All secrets** in Cloudflare Secrets (Wrangler), never in env vars or source code.

---

## PWA Config

**`manifest.json`:**
```json
{
  "name": "Circle",
  "short_name": "Circle",
  "start_url": "/",
  "display": "standalone",
  "background_color": "#0f0e0d",
  "theme_color": "#0f0e0d",
  "icons": [
    { "src": "/icon-192.png", "sizes": "192x192", "type": "image/png", "purpose": "any maskable" },
    { "src": "/icon-512.png", "sizes": "512x512", "type": "image/png", "purpose": "any maskable" }
  ]
}
```

**Service Worker:** Cache-first for app shell (HTML, WASM, CSS). Network-first for `/api/*`.

```javascript
const SHELL = ['/', '/index.html', '/app.wasm', '/style.css'];
self.addEventListener('install', e => e.waitUntil(caches.open('v1').then(c => c.addAll(SHELL))));
self.addEventListener('fetch', e => {
  if (e.request.url.includes('/api/')) return; // network only
  e.respondWith(caches.match(e.request).then(r => r || fetch(e.request)));
});
```

---

## Deployment: `wrangler.toml`

```toml
name = "circle"
main = "build/worker.wasm"
compatibility_date = "2024-04-01"
compatibility_flags = ["nodejs_compat"]

[[d1_databases]]
binding = "DB"
database_name = "circle-db"
database_id = "<d1-id>"

[[r2_buckets]]
binding = "MEDIA"
bucket_name = "circle-media"

[site]
bucket = "./dist"
```

## CI/CD: GitHub Actions

```yaml
name: Deploy
on:
  push:
    branches: [main]
jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Setup
        run: |
          rustup target add wasm32-unknown-unknown
          cargo install wasm-pack wasm-opt
      - name: Build
        run: |
          wasm-pack build --target web --release
          cargo build --target wasm32-unknown-unknown --release
          wasm-opt -Oz -o dist/app_bg.wasm dist/app_bg.wasm
      - name: Deploy
        run: npx wrangler deploy
        env:
          CLOUDFLARE_API_TOKEN: ${{ secrets.CF_API_TOKEN }}
```

---

## Crates

| Purpose | Crate |
|---|---|
| Full-stack framework | `leptos`, `leptos_axum` |
| Workers runtime | `worker` (cloudflare/workers-rs) |
| Auth | `webauthn-rs` |
| JWT | `jsonwebtoken` |
| Serialization | `serde`, `serde_json` |
| UUIDs | `uuid` (v4) |
| Time | `chrono` |
| Push | `web-push` |
| Errors | `thiserror`, `anyhow` |
| Async (client) | `wasm-bindgen-futures` |
| Async (server) | `tokio` |
| WASM optimizer | `wasm-opt` (CLI, Binaryen) |

---

## Known Bottlenecks & Fixes

| Problem | Cause | Fix |
|---|---|---|
| WASM binary too large | Default release build | `opt-level="z"`, `lto=true`, `panic="abort"`, then `wasm-opt -Oz` |
| iOS push not received | PWA not installed to home screen | Onboarding must detect and prompt `beforeinstallprompt` / iOS install instructions |
| D1 query slow | Missing index | Always index `circle_id + created_at DESC` on `posts`; all other lookups by PK |
| Worker CPU limit exceeded (10ms) | Heavy computation in request path | Move anything >5ms to background via `ctx.wait_until()` — e.g., push fan-out |
| R2 signed URL expired | Client cached stale URL | Re-fetch signed URL on 403 response from R2; never cache R2 URLs client-side |
| Refresh token replay | Token not rotated | Delete old refresh token from D1 on use; issue new one atomically |
| Push subscription stale | User cleared browser data | On 410 Gone from push endpoint, delete `push_sub` from D1 for that user |
| D1 write contention | Burst of simultaneous writes | D1 serializes writes per database — acceptable at this scale; no fix needed |
| WASM parse time on first load | Large binary, cold browser | Islands architecture limits WASM to interactive components only; Service Worker caches binary after first load |
| Leptos server fn not available in Workers | `leptos_axum` assumes Tokio | Use `leptos` with `workers-rs` router directly for server fn dispatch in Workers environment |

---

## Scaling Triggers (All Optional)

| Trigger | Action | Cost |
|---|---|---|
| >100K req/day | Workers Paid | $5/mo |
| >10GB photos | R2 Paid | $0.015/GB/mo |
| >5GB database | D1 Paid | $0.001/GB/mo |
| Realtime needed | Add Durable Objects | $0 (free tier: 1M req/day) |
| Video (v2) | Cloudflare Stream or encode→R2 | ~$5/mo |
| iOS push unreliable | Wrap in Capacitor for App Store | $99/yr Apple account |

No infrastructure migration ever required. Same stack from 5 to 500,000 users.


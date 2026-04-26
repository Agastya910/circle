# Circle — Changelog

---

## v1.0.0 — Initial scaffold (2026-04-24)

### What this project is

Circle is a self-hosted, invite-only social app for small groups of trusted friends. It is explicitly designed to remove every pattern that makes commercial social media extractive: no algorithm, no feed ranking, no engagement metrics, no discoverability, no profiles-as-performance. Posts appear in reverse-chronological order. When there is nothing new, the app says so and means it.

The single design goal is a reliable **notification channel for life updates from specific people**, not a destination people are engineered to linger in.

---

### Design decisions made in this session (important for future work)

**CSR over SSR (intentional v1 tradeoff).**
`leptos_axum` inside Cloudflare Workers is still unstable as of early 2026 because `leptos_axum` assumes Tokio but the Workers runtime is not Tokio. v1 ships as Leptos CSR on Cloudflare Pages + `workers-rs` API on Workers. The client/server boundary is already clean so SSR is a drop-in upgrade once the `workers-rs` + `leptos_axum` story stabilises.

**Session tokens over passkeys / JWT (intentional v1 tradeoff).**
`webauthn-rs` requires `rustcrypto` features to compile to `wasm32-unknown-unknown` (the default OpenSSL backend doesn't). Rather than debug that at scaffold time, auth is invite-code → 32-byte hex session token, 30-day TTL, stored in D1. A clean seam is left for passkey swap-in once the app shell is live and testable on a real phone.

**Tickle pushes over encrypted payloads (intentional v1 tradeoff).**
Web Push payload encryption (RFC 8291, AES-GCM) requires deriving keys from the subscription's `p256dh` point, which is non-trivial WASM crypto. v1 sends an empty-body "tickle": the service worker wakes, shows a generic "Someone in your circle posted" notification, and the tap opens the app which fetches fresh posts. Encrypted payloads with poster name/preview are v1.1.

**No Firebase / Google services.**
Push notifications use VAPID (direct Web Push) with P-256 ES256 JWTs signed in the worker via the `p256` crate. No FCM, no external analytics, no tracking.

**No image upload UI in v1 (scaffolded only).**
The back-end media pipeline is complete (upload-url endpoint, R2 put/get handlers, ownership validation). The Compose component has a placeholder but the client-side WebP encode → upload flow is not wired up. This is the recommended first extension.

**Reactions are a fixed six-emoji allowlist.**
❤️ 😂 🔥 👏 🙏 😢. Toggle semantics (second tap removes). No free-form emoji, no like counts shown prominently, no public tallies.

**Comments exist but have no UI.**
Worker handlers for list/add comments are implemented and tested-compilable. The Leptos component side is stubbed. Implementing the comments drawer is the second recommended extension after image upload.

---

### Tech stack (all $0/month, domain excluded)

| Layer | Technology |
|---|---|
| Frontend | Leptos 0.7 CSR (Rust → WASM) |
| Hosting | Cloudflare Pages |
| API | `workers-rs` (Rust → WASM on Cloudflare Workers) |
| Database | Cloudflare D1 (serverless SQLite) |
| Media | Cloudflare R2 (zero egress) |
| Push | Web Push VAPID (direct, no Firebase) |
| Auth | Invite code + session token (passkeys planned v1.1) |
| CI/CD | GitHub Actions + `wrangler deploy` |
| TLS / WAF | Cloudflare Free Plan |

---

### What was built

#### Workspace layout
```
circle/
├── shared/          # Serde types shared between client and worker
├── worker/          # Cloudflare Worker API (Rust → WASM)
│   └── src/
│       ├── lib.rs           # Router, CORS, security headers
│       ├── auth.rs          # Session create/validate/delete
│       ├── db.rs            # D1 binding helpers
│       ├── push.rs          # VAPID JWT + fan-out
│       ├── util.rs          # CORS headers, json_error, new_id, now_secs
│       └── handlers/
│           ├── auth.rs      # register, me, logout
│           ├── posts.rs     # list, create, comments, add_comment
│           ├── reactions.rs # toggle (allowlist-gated)
│           ├── media.rs     # upload_url, put_media, get_media
│           └── push.rs      # subscribe, vapid_public
├── client/          # Leptos 0.7 CSR PWA
│   └── src/
│       ├── main.rs          # mount
│       ├── api.rs           # gloo-net HTTP client, API_BASE from env
│       ├── state.rs         # AuthCtx RwSignal, LocalStorage persistence
│       └── components.rs    # App, Login, Home, Compose, PostView, push utils
├── schema.sql       # D1 schema (users, sessions, posts, reactions, comments, invite_codes)
├── wrangler.toml    # Worker config, D1 + R2 bindings, env vars
└── README.md        # Setup, dev loop, deployment, v1→v1.1 punch list
```

#### API surface

| Method | Path | Auth | Description |
|---|---|---|---|
| POST | `/api/auth/register` | — | Claim invite code, create user, return token |
| POST | `/api/auth/logout` | Bearer | Delete session |
| GET | `/api/me` | Bearer | Current user |
| GET | `/api/posts` | Bearer | 50 posts, reverse-chrono, with reactions + comment counts |
| POST | `/api/posts` | Bearer | Create post, fan-out tickle pushes |
| GET | `/api/posts/:id/comments` | Bearer | Comments oldest-first |
| POST | `/api/posts/:id/comments` | Bearer | Add comment |
| POST | `/api/reactions` | Bearer | Toggle emoji reaction |
| POST | `/api/upload-url` | Bearer | Get R2 upload key |
| PUT | `/api/media/:key` | Bearer | Upload to R2 (max 2 MB, ownership-gated) |
| GET | `/api/media/:key` | Bearer | Fetch from R2 (post-reference or ownership gated) |
| POST | `/api/push/subscribe` | Bearer | Store push subscription |
| GET | `/api/push/vapid` | — | VAPID public key |

#### Database schema (D1 / SQLite)
- `users` — id, circle_id, display_name, avatar_key, push_sub (JSON), created_at
- `sessions` — token (PK), user_id, expires_at, created_at
- `posts` — id, circle_id, author_id, body, image_key, created_at
- `reactions` — id, post_id, user_id, emoji, created_at; UNIQUE(post_id, user_id, emoji)
- `comments` — id, post_id, author_id, body, created_at
- `invite_codes` — code (PK), used_by, used_at
- Key indexes: `posts(circle_id, created_at)`, `reactions(post_id)`, `comments(post_id)`, `sessions(user_id)`

#### Client (Leptos CSR PWA)
- Single-page, no router — Auth guard shows Login or Home
- Login: invite code + display name form
- Home: Compose box + reverse-chrono feed + "you're caught up" empty state
- PostView: author avatar initial, relative timestamp, body, image (if present), 6 reaction buttons
- Push: 🔔 button → requests Notification permission → subscribes → POSTs to `/api/push/subscribe`
- Service worker (`sw.js`): cache-first shell (HTML/CSS/WASM/JS), network-only `/api/*`, handles push events
- PWA manifest: standalone display, dark theme `#0f0e0d`, maskable icons

#### Secrets required before deploy
```
SESSION_SECRET        # 32+ random bytes, base64
VAPID_PUBLIC_KEY      # from: npx web-push generate-vapid-keys
VAPID_PRIVATE_KEY     # from same command (privateKey field, base64url raw scalar)
VAPID_SUBJECT         # mailto:you@example.com
```

---

### Known gaps / v1.1 punch list (in priority order)

1. **Image upload UI** — back-end complete, client Compose component needs canvas→WebP encode + PUT wired up
2. **Comments drawer** — worker handlers exist, Leptos side needs implementing
3. **Encrypted push payloads** (RFC 8291) — send poster name/preview in notification body
4. **Passkeys / WebAuthn** — swap `webauthn-rs` in once testable on device; seam is in `handlers/auth.rs`
5. **SSR** — drop-in once `leptos_axum` + `workers-rs` Tokio compat stabilises
6. **Avatar upload** — same pipeline as post images; `avatar_key` column already in schema
7. **Admin endpoint** — generate invite codes without touching D1 directly

---

### Key constants / configuration notes

- `SESSION_TTL_SECS` = 2,592,000 (30 days), `worker/src/auth.rs`
- `API_BASE` = compile-time env var in client (`api.rs`), defaults to `http://127.0.0.1:8787` for local dev
- Max upload size = 2 MB, enforced in `handlers/media.rs`
- Post fetch limit = 50, hardcoded in `handlers/posts.rs`
- Allowed emojis = `["❤️","😂","🔥","👏","🙏","😢"]`, `handlers/reactions.rs`
- `CIRCLE_ID` env var defaults to `"default"` — all users share one circle unless changed

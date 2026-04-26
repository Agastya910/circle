# Circle — Changelog

---

## v1.2.0 — Self-service invites + media upload bug fix (2026-04-25)

### What changed

**Self-service invites via portfolio.**
Friends can now mint their own invite codes by visiting a page on the portfolio (`/circle`) and entering the current passphrase. The passphrase is rotated weekly by Agastya from `/circle/admin` using the worker's `ADMIN_SECRET` as the auth gate. New worker endpoints: `POST /api/invite/redeem` (public, validates passphrase, mints a fresh single-use code) and `POST /api/admin/passphrase` (Bearer-auth, rotates the active phrase). State lives in a new `admin_settings` D1 table ([migrations/0003_admin_settings.sql](migrations/0003_admin_settings.sql)). Portfolio side is a thin Next.js proxy — no admin secret ever ships to the browser bundle.

**Bug fix: image/video upload returned HTTP 500.**
The per-user storage-cap code introduced in v1.1 bound `bytes.len() as i64` to the D1 `UPDATE users SET media_bytes = …` statement. workers-rs serialises Rust `i64` as a JS BigInt, which D1 rejects with `D1_TYPE_ERROR: Type 'bigint' not supported`. Switched the column read/write and the constant to `f64` to match the rest of the codebase ([worker/src/handlers/media.rs](worker/src/handlers/media.rs)). f64 mantissa is 53 bits so all values up to ~9 PB round-trip exactly.

**ADMIN_SECRET source-of-truth.**
Removed the `ADMIN_SECRET = "dev-admin-secret"` line from `wrangler.toml [vars]` so it can no longer shadow the real secret on deploy. Handlers now prefer `env.secret("ADMIN_SECRET")` and fall back to `env.var()` for `wrangler dev` convenience. Refactored `create_invite` to share the same `check_admin` helper as `set_passphrase`.

---

## v1.1.0 — Cross-browser video, Cloudflare deployment, and storage caps (2026-04-25)

### Live URLs
- **App**: https://circle-app-59w.pages.dev
- **API**: https://circle-api.agastyatodi.workers.dev

### What changed

**Cross-browser video compression.**
The original `client/video.js` recorder only emitted WebM, which iOS Safari neither produces (no WebM `MediaRecorder` support) nor plays back. The MIME-candidate list now tries `video/mp4;codecs=avc1.42E01E,mp4a.40.2` (H.264 baseline + AAC-LC) first and falls back to WebM/VP9/VP8 for Chromium and Firefox. The chosen extension is propagated end-to-end so each blob lands in R2 with the right `Content-Type`:
- `shared::UploadUrlRequest` gained an optional `ext` field
- The Compose component reads `blob.type()` and forwards `mp4` or `webm`
- `worker/src/handlers/media.rs` validates `ext` per kind, names the R2 key with the right suffix, and serves both `video/webm` and `video/mp4` on read

**Per-user storage cap.**
Each user is now capped at **500 MB** of cumulative R2 storage (≈100 videos at the 5 MB-per-clip ceiling, or 250 photos at 2 MB each). With ten users this consumes at most half of R2's 10 GB free tier. Implementation:
- New `users.media_bytes INTEGER NOT NULL DEFAULT 0` column ([migrations/0002_user_media_bytes.sql](migrations/0002_user_media_bytes.sql))
- `put_media` reads the running total, rejects with HTTP 413 if the new upload would exceed the cap, and increments the counter on success
- The check is post-compression, so the on-device WebP/MP4 encoders are doing the lifting. There is no separate per-day or per-post rate limit — the cumulative cap is the only governor.

**Cloudflare deployment (live).**
- D1 `circle-db` (`b1e8176f-…`) seeded with [schema.sql](schema.sql) (6 tables) + 11 invite codes
- R2 bucket `circle-media` bound for media storage
- Worker `circle-api` deployed to `https://circle-api.agastyatodi.workers.dev`
- Pages project `circle-app` deployed to `https://circle-app-59w.pages.dev`
- All required secrets set in the worker: `SESSION_SECRET`, `ADMIN_SECRET`, `VAPID_PUBLIC_KEY`, `VAPID_PRIVATE_KEY`, `VAPID_SUBJECT`
- `wrangler.toml` `ALLOWED_ORIGIN` points at the Pages production URL; CORS preflight verified

**Repo hygiene.**
- Project-local git initialized (a stray `~/.git` from an editor accident was removed)
- `.gitignore` excludes `target/`, `client/dist/`, `worker/build/`, `.env*`, `.wrangler/`, `.claude/`, editor folders
- Local-only `.env` written with `CF_ACCOUNT_ID`, `API_BASE`, `PAGES_URL`, `FIRST_INVITE_CODE`, `ADMIN_SECRET`; `CF_API_TOKEN` left blank for the operator to mint
- Initial commit `a4a23ec`; remote `origin` set to https://github.com/Agastya910/circle.git but not pushed

### Files touched
- `client/video.js` — added MP4/H.264/AAC candidates ahead of WebM
- `shared/src/lib.rs` — `UploadUrlRequest.ext: Option<String>`
- `client/src/api.rs` — `get_upload_url(token, kind, ext)` signature
- `client/src/components.rs` — extracts `ext` from blob MIME before requesting upload URL
- `worker/src/handlers/media.rs` — `ext` allowlist, MP4 read-side mapping, per-user cap check + counter increment
- `schema.sql` — `users.media_bytes` column added
- `migrations/0002_user_media_bytes.sql` — new
- `worker/wrangler.toml` — `ALLOWED_ORIGIN` set to Pages URL
- `.gitignore`, `.env` — new

### What is NOT in this release
- **GitHub Actions auto-deploy.** The workflow exists but the repo-side secrets/vars (`CF_API_TOKEN`, `CF_ACCOUNT_ID`, `API_BASE`) are not set, so push-to-deploy is dormant. Manual deploy works via `wrangler deploy` (worker) and `wrangler pages deploy client/dist --project-name=circle-app`.
- **Per-user upload deletion / pruning.** The cap only grows; users will need to wait for a future "delete post" feature (or manual D1 surgery) to free space.
- **Encrypted push payloads, passkeys, comments drawer, SSR, avatar upload.** Same as v1.0.

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

-- Circle v1 schema. Run: wrangler d1 execute circle-db --file=schema.sql

CREATE TABLE IF NOT EXISTS users (
    id           TEXT PRIMARY KEY,
    circle_id    TEXT NOT NULL,
    display_name TEXT NOT NULL,
    avatar_key   TEXT,
    pin_hash     TEXT,
    pin_salt     TEXT,
    push_sub     TEXT,
    media_bytes  INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    token      TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS posts (
    id         TEXT PRIMARY KEY,
    circle_id  TEXT NOT NULL,
    author_id  TEXT NOT NULL REFERENCES users(id),
    body       TEXT,
    image_key  TEXT,
    video_key  TEXT,
    media_keys TEXT,    -- JSON array of R2 keys, max 4 images
    deleted_at INTEGER, -- NULL = live; unix secs when soft-deleted
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS reactions (
    id         TEXT PRIMARY KEY,
    post_id    TEXT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL REFERENCES users(id),
    emoji      TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(post_id, user_id, emoji)
);

CREATE TABLE IF NOT EXISTS comments (
    id         TEXT PRIMARY KEY,
    post_id    TEXT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    author_id  TEXT NOT NULL REFERENCES users(id),
    body       TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS invite_codes (
    code       TEXT PRIMARY KEY,
    circle_id  TEXT NOT NULL,
    used_by    TEXT,
    used_at    INTEGER,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS admin_settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_posts_circle_time ON posts(circle_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_reactions_post ON reactions(post_id);
CREATE INDEX IF NOT EXISTS idx_comments_post ON comments(post_id);
CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);

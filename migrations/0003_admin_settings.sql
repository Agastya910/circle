-- Single-row settings table holding the current invite passphrase
-- and (room for) future runtime-tunable admin values.
-- Apply via: wrangler d1 execute circle-db --remote --file=migrations/0003_admin_settings.sql
CREATE TABLE IF NOT EXISTS admin_settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

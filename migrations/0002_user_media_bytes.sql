-- Adds per-user cumulative media-storage counter for the upload cap enforced
-- in worker/src/handlers/media.rs.
-- Apply via: wrangler d1 execute circle-db --remote --file=migrations/0002_user_media_bytes.sql
ALTER TABLE users ADD COLUMN media_bytes INTEGER NOT NULL DEFAULT 0;

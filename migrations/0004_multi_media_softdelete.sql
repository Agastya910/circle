-- Migration 0004: multi-image posts and soft delete
ALTER TABLE posts ADD COLUMN media_keys TEXT;   -- JSON array of R2 keys, max 4 images
ALTER TABLE posts ADD COLUMN deleted_at INTEGER; -- NULL = live, set to unix secs on delete (soft)

-- Optimistic lock. Plan 3 deferred this because the mesh was the only writer;
-- once a customer can add lines from the app, a double-tap is a lost update.
ALTER TABLE omnideliv.baskets
    ADD COLUMN IF NOT EXISTS version BIGINT NOT NULL DEFAULT 0;

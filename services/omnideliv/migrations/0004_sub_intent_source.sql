-- Where a sub-intent came from. `mesh` is the Concierge's decomposition;
-- `browse` is the synthetic sub-intent that carries manually-added lines when
-- the customer is shopping without the agent.
--
-- Manual lines need a sub-intent because basket_lines.sub_intent_id is NOT NULL
-- and is the partition key Basket::apply scopes by. Giving browsing its own
-- sub-intent keeps that partitioning intact rather than making the column
-- nullable, which would weaken the single-writer guarantee for everyone.
ALTER TABLE omnideliv.sub_intents
    ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'mesh'
        CHECK (source IN ('mesh', 'browse'));

-- One browse sub-intent per vertical per basket — the find-or-create in
-- Basket::browse_sub_intent relies on this being enforced, not merely intended.
CREATE UNIQUE INDEX IF NOT EXISTS uq_browse_sub_intent
    ON omnideliv.sub_intents (basket_id, vertical)
    WHERE source = 'browse';

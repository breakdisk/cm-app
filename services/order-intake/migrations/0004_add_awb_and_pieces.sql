-- Migration: 0004 — Order Intake: structured AWB column + shipment_pieces table
--
-- Adds the structured AWB value (replaces plain tracking_number) and the
-- piece-level records that track individual parcels within a shipment.
-- tracking_number is kept for backward-compatibility and given the awb value.

-- ── Step 1: add awb column alongside tracking_number ─────────────────────────
ALTER TABLE order_intake.shipments
    ADD COLUMN IF NOT EXISTS awb         TEXT,
    ADD COLUMN IF NOT EXISTS piece_count SMALLINT NOT NULL DEFAULT 1
        CHECK (piece_count BETWEEN 1 AND 999);

-- Back-fill from existing tracking_number (may already be structured AWBs)
UPDATE order_intake.shipments
    SET awb = tracking_number
    WHERE awb IS NULL;

-- Make awb NOT NULL + UNIQUE now that it is populated
ALTER TABLE order_intake.shipments
    ALTER COLUMN awb SET NOT NULL;

ALTER TABLE order_intake.shipments
    ADD CONSTRAINT shipments_awb_unique UNIQUE (awb);

-- Keep tracking_number in sync going forward
CREATE OR REPLACE FUNCTION order_intake.sync_awb_to_tracking()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.awb IS NOT NULL AND NEW.tracking_number IS DISTINCT FROM NEW.awb THEN
        NEW.tracking_number := NEW.awb;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_sync_awb_to_tracking
    BEFORE INSERT OR UPDATE OF awb ON order_intake.shipments
    FOR EACH ROW EXECUTE FUNCTION order_intake.sync_awb_to_tracking();

-- ── Step 2: shipment_pieces ───────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS order_intake.shipment_pieces (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    shipment_id         UUID        NOT NULL
                                    REFERENCES order_intake.shipments(id)
                                    ON DELETE CASCADE,
    tenant_id           UUID        NOT NULL,

    -- Piece identity
    piece_number        SMALLINT    NOT NULL CHECK (piece_number BETWEEN 1 AND 999),
    piece_awb           TEXT        NOT NULL,   -- e.g. LS-PH1-S0001234X-001

    -- Weight
    declared_weight_g   INTEGER     NOT NULL CHECK (declared_weight_g > 0),
    actual_weight_g     INTEGER     CHECK (actual_weight_g > 0),

    -- Dimensions (optional — for volumetric billing)
    length_cm           INTEGER,
    width_cm            INTEGER,
    height_cm           INTEGER,

    -- Contents description (e.g. "Clothes, Electronics")
    description         TEXT,

    -- Scan / location tracking
    status              TEXT        NOT NULL DEFAULT 'pending'
                                    CHECK (status IN (
                                        'pending','picked_up','at_hub',
                                        'in_transit','out_for_delivery',
                                        'delivered','failed','returned',
                                        'exception'
                                    )),
    last_hub_id         UUID,
    last_scanned_at     TIMESTAMPTZ,

    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Each piece AWB is globally unique
    CONSTRAINT pieces_awb_unique UNIQUE (piece_awb),
    -- Piece numbers within a shipment are unique
    CONSTRAINT pieces_shipment_seq_unique UNIQUE (shipment_id, piece_number)
);

CREATE INDEX idx_pieces_shipment   ON order_intake.shipment_pieces (shipment_id);
CREATE INDEX idx_pieces_awb        ON order_intake.shipment_pieces (piece_awb);
CREATE INDEX idx_pieces_status     ON order_intake.shipment_pieces (tenant_id, status);
CREATE INDEX idx_pieces_hub        ON order_intake.shipment_pieces (last_hub_id)
    WHERE last_hub_id IS NOT NULL;

-- Auto-update updated_at
CREATE OR REPLACE FUNCTION order_intake.set_piece_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN NEW.updated_at = NOW(); RETURN NEW; END;
$$;

CREATE TRIGGER trg_pieces_updated_at
    BEFORE UPDATE ON order_intake.shipment_pieces
    FOR EACH ROW EXECUTE FUNCTION order_intake.set_piece_updated_at();

-- RLS
ALTER TABLE order_intake.shipment_pieces ENABLE ROW LEVEL SECURITY;
ALTER TABLE order_intake.shipment_pieces FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON order_intake.shipment_pieces
    USING (tenant_id = current_setting('app.tenant_id')::uuid);

-- ── Step 3: AWB sequence counters table (used by Redis fallback generator) ────
--
-- One row per (tenant_code, service_char). The Postgres fallback generator
-- uses a PostgreSQL SEQUENCE per tenant+service, seeded here via dynamic SQL.
-- This table tracks the current high-water mark for audit / DR purposes.

CREATE TABLE IF NOT EXISTS order_intake.awb_sequences (
    tenant_code         CHAR(3)     NOT NULL,
    service_char        CHAR(1)     NOT NULL CHECK (service_char IN ('S','E','D','B','N')),
    last_issued         INTEGER     NOT NULL DEFAULT 0
                                    CHECK (last_issued BETWEEN 0 AND 9999999),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_code, service_char)
);

COMMENT ON TABLE order_intake.awb_sequences IS
    'High-water mark for issued AWB sequences per tenant/service. '
    'The live counter lives in Redis (awb:seq:{tenant}:{char}). '
    'This table is updated on Redis failover and on periodic reconciliation.';

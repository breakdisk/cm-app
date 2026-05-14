-- Marketplace: vehicle listings and spot bookings (ADR-0013)

CREATE TABLE IF NOT EXISTS carrier.vehicle_listings (
    id                              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id                       UUID NOT NULL,
    carrier_id                      UUID NOT NULL REFERENCES carrier.carriers(id) ON DELETE CASCADE,
    vehicle_plate                   TEXT NOT NULL,
    size_class                      TEXT NOT NULL,
    max_weight_kg                   REAL NOT NULL DEFAULT 0,
    max_volume_m3                   REAL,
    base_price_cents                BIGINT NOT NULL DEFAULT 0,
    per_km_cents                    BIGINT NOT NULL DEFAULT 0,
    per_kg_cents                    BIGINT,
    service_area_label              TEXT NOT NULL DEFAULT '',
    idle_from                       TIMESTAMPTZ NOT NULL,
    idle_until                      TIMESTAMPTZ NOT NULL,
    status                          TEXT NOT NULL DEFAULT 'active',
    carrier_response_window_mins    INT NOT NULL DEFAULT 15,
    bookings_today                  BIGINT NOT NULL DEFAULT 0,
    revenue_today_cents             BIGINT NOT NULL DEFAULT 0,
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_vehicle_listings_carrier_id ON carrier.vehicle_listings(carrier_id);
CREATE INDEX IF NOT EXISTS idx_vehicle_listings_status     ON carrier.vehicle_listings(status);

CREATE TABLE IF NOT EXISTS carrier.marketplace_bookings (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID NOT NULL,
    listing_id          UUID NOT NULL REFERENCES carrier.vehicle_listings(id),
    carrier_id          UUID NOT NULL REFERENCES carrier.carriers(id),
    shipment_id         UUID NOT NULL,
    awb                 TEXT NOT NULL,
    consumer_name       TEXT NOT NULL DEFAULT '',
    consumer_phone      TEXT,
    pickup_label        TEXT NOT NULL DEFAULT '',
    dropoff_label       TEXT NOT NULL DEFAULT '',
    cargo_weight_kg     REAL NOT NULL DEFAULT 0,
    cargo_volume_m3     REAL,
    quoted_price_cents  BIGINT NOT NULL DEFAULT 0,
    status              TEXT NOT NULL DEFAULT 'pending',
    pickup_at           TIMESTAMPTZ NOT NULL,
    picked_up_at        TIMESTAMPTZ,
    picked_up_by        TEXT,
    pickup_notes        TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_marketplace_bookings_carrier_id  ON carrier.marketplace_bookings(carrier_id);
CREATE INDEX IF NOT EXISTS idx_marketplace_bookings_listing_id  ON carrier.marketplace_bookings(listing_id);
CREATE INDEX IF NOT EXISTS idx_marketplace_bookings_status      ON carrier.marketplace_bookings(status);

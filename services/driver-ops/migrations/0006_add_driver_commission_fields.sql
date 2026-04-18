-- Partner-portal driver management fields.
-- driver_type is constrained; zone / vehicle_type are free-form so ops can extend without migrations.
-- per_delivery_rate_cents — integer cents (matches cod_amount_cents convention).
-- cod_commission_rate_bps — basis points (250 = 2.50%) — integer storage avoids NUMERIC/Decimal dep.
ALTER TABLE driver_ops.drivers
    ADD COLUMN IF NOT EXISTS driver_type             TEXT    NOT NULL DEFAULT 'full_time'
        CHECK (driver_type IN ('full_time','part_time')),
    ADD COLUMN IF NOT EXISTS per_delivery_rate_cents INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS cod_commission_rate_bps INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS zone                    TEXT,
    ADD COLUMN IF NOT EXISTS vehicle_type            TEXT;

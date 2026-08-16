-- Which identity user operates this vendor.
--
-- Nullable: vendors seeded or onboarded by the Partner have no portal user yet,
-- and making this NOT NULL would mean inventing one at creation. The /me
-- endpoints simply find nothing for those, which is the honest answer.
ALTER TABLE omnideliv.vendors
    ADD COLUMN IF NOT EXISTS user_id UUID;

-- One vendor per portal user per tenant. Partial so the many NULLs do not
-- collide — a unique index would otherwise allow only one unclaimed vendor.
CREATE UNIQUE INDEX IF NOT EXISTS uq_vendor_user
    ON omnideliv.vendors (tenant_id, user_id)
    WHERE user_id IS NOT NULL;

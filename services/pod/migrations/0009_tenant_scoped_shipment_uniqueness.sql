-- Shipment uniqueness is per tenant, not global.
--
-- uq_pod_shipment (0001) and uq_pop_shipment_submitted (0003) were unique on
-- shipment_id alone, so a row written under one tenant for another tenant's
-- shipment id blocked the real owner at the database. Who wrote a row first must
-- never decide ownership: the booking record's tenant does, and pod checks it
-- against order-intake before it opens evidence.
--
-- The new indexes are strictly weaker than the ones they replace (every set of
-- rows that is unique on shipment_id is unique on (tenant_id, shipment_id)), so
-- they always build on existing data. Both upserts conflict on (id), not on
-- these indexes, so images still running the old code keep working during the
-- rolling deploy: nothing names an index.
CREATE UNIQUE INDEX IF NOT EXISTS uq_pod_tenant_shipment
    ON pod.proofs (tenant_id, shipment_id)
    WHERE status IN ('draft', 'submitted', 'verified');

CREATE UNIQUE INDEX IF NOT EXISTS uq_pop_tenant_shipment_submitted
    ON pod.proofs_of_pickup (tenant_id, shipment_id)
    WHERE status = 'submitted';

DROP INDEX IF EXISTS pod.uq_pod_shipment;
DROP INDEX IF EXISTS pod.uq_pop_shipment_submitted;

-- services/hub-ops/migrations/0012_consolidation_status_and_loadings.sql

ALTER TABLE hub_ops.consolidation_plans
  ADD COLUMN status TEXT NOT NULL DEFAULT 'draft';

CREATE TABLE hub_ops.consolidation_plan_loadings (
  id         UUID        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
  tenant_id  UUID        NOT NULL,
  plan_id    UUID        NOT NULL REFERENCES hub_ops.consolidation_plans(id),
  awb        TEXT        NOT NULL,
  scanned_by UUID,
  scanned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (plan_id, awb)
);

CREATE INDEX ON hub_ops.consolidation_plan_loadings (plan_id);

-- Migration: 0006 — extend container status CHECK with 'deconsolidated'
--
-- The cross-border transfer flow adds `Deconsolidated` as the terminal Container
-- state (libs/types ContainerStatus). `Delivered` is retained for backward compat.

ALTER TABLE hub_ops.containers
    DROP CONSTRAINT IF EXISTS containers_status_check;

ALTER TABLE hub_ops.containers
    ADD CONSTRAINT containers_status_check
    CHECK (status IN (
        'planning','manifested','loading','sealed','in_transit',
        'arrived_at_port','customs','released','deconsolidated','delivered'
    ));

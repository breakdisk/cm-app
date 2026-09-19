-- Migration: 0015 — the whole-home moving catalogue
--
-- What a room may hold, with each item's packed volume (litres) and weight
-- (kg per unit), and whether it is offered for dismantle and rebuild or
-- special packing by default. Tenant config: a rate or preset change must
-- not need an app release.
--
-- Rows with tenant_id NULL are the platform defaults, seeded from the design
-- handoff. A tenant that adds rows for a group replaces the defaults for that
-- group only; groups it has not touched keep the defaults.

CREATE TABLE IF NOT EXISTS order_intake.home_catalogue (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID,
    group_key   TEXT        NOT NULL,
    item_key    TEXT        NOT NULL,
    name        TEXT        NOT NULL,
    volume_l    INTEGER     NOT NULL CHECK (volume_l > 0),
    weight_kg   INTEGER     NOT NULL CHECK (weight_kg > 0),
    assembly    BOOLEAN     NOT NULL DEFAULT FALSE,
    packing     BOOLEAN     NOT NULL DEFAULT FALSE,
    sort_order  INTEGER     NOT NULL DEFAULT 0,
    active      BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE NULLS NOT DISTINCT (tenant_id, group_key, item_key)
);

CREATE INDEX IF NOT EXISTS idx_home_catalogue_tenant ON order_intake.home_catalogue (tenant_id, group_key);

INSERT INTO order_intake.home_catalogue (group_key, item_key, name, volume_l, weight_kg, assembly, packing, sort_order)
VALUES
    ('living', '3_seater_sofa', '3-seater sofa', 2100, 78, true, false, 0),
    ('living', 'armchair', 'Armchair', 700, 32, false, false, 1),
    ('living', 'tv_unit', 'TV unit', 900, 44, true, false, 2),
    ('living', 'television_65in', 'Television 65"', 300, 28, false, true, 3),
    ('living', 'coffee_table', 'Coffee table', 400, 22, true, false, 4),
    ('living', 'curtain_fixture', 'Curtain fixture', 120, 8, true, false, 5),
    ('living', 'rug_rolled', 'Rug, rolled', 200, 14, false, false, 6),
    ('living', 'packed_box', 'Packed box', 200, 18, false, false, 7),
    ('kitchen', 'fridge_american', 'Fridge, American', 1100, 116, false, true, 0),
    ('kitchen', 'washing_machine', 'Washing machine', 500, 72, false, false, 1),
    ('kitchen', 'dining_table', 'Dining table', 1200, 58, true, false, 2),
    ('kitchen', 'dining_chair', 'Dining chair', 200, 9, false, false, 3),
    ('kitchen', 'packed_box', 'Packed box', 200, 18, false, false, 4),
    ('bed', 'king_bed', 'King bed', 2600, 96, true, false, 0),
    ('bed', 'single_bed', 'Single bed', 1200, 44, true, false, 1),
    ('bed', 'bunk_bed', 'Bunk bed', 1800, 62, true, false, 2),
    ('bed', 'wardrobe_3_door', 'Wardrobe, 3-door', 2400, 104, true, false, 3),
    ('bed', 'dresser', 'Dresser', 900, 48, false, false, 4),
    ('bed', 'nightstand', 'Nightstand', 250, 12, false, false, 5),
    ('bed', 'desk', 'Desk', 700, 30, true, false, 6),
    ('bed', 'curtain_fixture', 'Curtain fixture', 120, 8, true, false, 7),
    ('bed', 'mirror_full_length', 'Mirror, full length', 150, 16, false, true, 8),
    ('bed', 'packed_box', 'Packed box', 200, 18, false, false, 9),
    ('util', 'shelving_unit', 'Shelving unit', 700, 34, true, false, 0),
    ('util', 'tool_chest', 'Tool chest', 600, 62, false, false, 1),
    ('util', 'bicycle', 'Bicycle', 400, 14, false, false, 2),
    ('util', 'packed_box', 'Packed box', 200, 18, false, false, 3),
    ('desks', 'workstation_desk', 'Workstation desk', 1100, 46, true, false, 0),
    ('desks', 'desk_pedestal', 'Desk pedestal', 300, 22, false, false, 1),
    ('desks', 'task_chair', 'Task chair', 400, 13, false, false, 2),
    ('desks', 'monitor', 'Monitor', 100, 6, false, true, 3),
    ('desks', 'curtain_fixture', 'Curtain fixture', 120, 8, true, false, 4),
    ('desks', 'screen_divider', 'Screen divider', 200, 11, true, false, 5),
    ('desks', 'archive_box', 'Archive box', 100, 14, false, false, 6),
    ('meeting', 'meeting_table_8_seat', 'Meeting table, 8-seat', 1800, 84, true, false, 0),
    ('meeting', 'meeting_chair', 'Meeting chair', 300, 10, false, false, 1),
    ('meeting', 'curtain_fixture', 'Curtain fixture', 120, 8, true, false, 2),
    ('meeting', 'whiteboard', 'Whiteboard', 150, 18, false, true, 3),
    ('meeting', 'display_screen_75in', 'Display screen 75"', 350, 42, false, true, 4),
    ('meeting', 'archive_box', 'Archive box', 100, 14, false, false, 5),
    ('reception', 'reception_counter', 'Reception counter', 1600, 96, true, false, 0),
    ('reception', 'lounge_sofa', 'Lounge sofa', 1400, 62, false, false, 1),
    ('reception', 'coffee_table', 'Coffee table', 400, 22, true, false, 2),
    ('reception', 'planter', 'Planter', 300, 26, false, false, 3),
    ('reception', 'curtain_fixture', 'Curtain fixture', 120, 8, true, false, 4),
    ('reception', 'signage_panel', 'Signage panel', 200, 19, false, true, 5),
    ('server', 'server_rack_42u', 'Server rack, 42U', 1400, 168, false, true, 0),
    ('server', 'ups_unit', 'UPS unit', 400, 88, false, true, 1),
    ('server', 'network_switch', 'Network switch', 100, 9, false, true, 2),
    ('server', 'cable_crate', 'Cable crate', 200, 20, false, false, 3),
    ('office_util', 'filing_cabinet_4_drawer', 'Filing cabinet, 4-drawer', 800, 68, false, false, 0),
    ('office_util', 'storage_cupboard', 'Storage cupboard', 1200, 74, true, false, 1),
    ('office_util', 'fridge_under_counter', 'Fridge, under-counter', 400, 38, false, false, 2),
    ('office_util', 'coffee_machine', 'Coffee machine', 200, 24, false, true, 3),
    ('office_util', 'archive_box', 'Archive box', 100, 14, false, false, 4)
ON CONFLICT (tenant_id, group_key, item_key) DO NOTHING;

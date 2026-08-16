-- Modifier groups on catalog items, and the customer's chosen options on a
-- basket line.
--
-- `catalog_items.modifiers` has existed since 0002 as an untyped JSONB column.
-- It was written by the API and read by nothing: no pricing, no validation, no
-- UI. This migration gives it a shape so it can start meaning something.
--
-- Shape (an array of groups):
--   [{ "id": uuid, "name": "Size", "min_select": 1, "max_select": 1,
--      "options": [{ "id": uuid, "name": "Large", "price_delta_cents": 2000 }] }]
--
-- min_select 0 makes a group optional; max_select 1 renders as radio buttons and
-- >1 as checkboxes. price_delta_cents is signed on purpose — "no cheese" is
-- allowed to take money off.

-- Any value that does not fit the shape above is reset. This is a rewrite of
-- data, so it is worth being explicit about what is lost: nothing readable. The
-- column has never been parsed by any code path, so a non-conforming value is
-- one no part of the system could act on, and leaving it in place would make the
-- item fail to load once the column is strictly typed. Rows already holding the
-- default '[]' are untouched, which is expected to be all of them.
UPDATE omnideliv.catalog_items
   SET modifiers = '[]'::jsonb
 WHERE jsonb_typeof(modifiers) IS DISTINCT FROM 'array'
    OR EXISTS (
         SELECT 1
           FROM jsonb_array_elements(modifiers) AS g
          WHERE jsonb_typeof(g) IS DISTINCT FROM 'object'
             OR g->>'id'   IS NULL
             OR g->>'name' IS NULL
             OR jsonb_typeof(g->'options') IS DISTINCT FROM 'array'
       );

-- What the customer actually chose, snapshotted onto the line.
--
-- A snapshot rather than a reference, for the same reason `unit_price_cents`
-- already is one: the customer pays what they were shown. A vendor renaming
-- "Large" or repricing it after the line was proposed must not retroactively
-- change what this basket owes, and an option deleted from the catalog must not
-- make an existing line unreadable.
--
-- Each element: { "group_id", "group_name", "option_id", "option_name",
--                 "price_delta_cents" }
--
-- The deltas here are already folded into `unit_price_cents`, so every existing
-- total keeps working untouched. They are stored so the line can be explained —
-- to the customer on a receipt, and to the vendor who has to make the thing.
ALTER TABLE omnideliv.basket_lines
    ADD COLUMN IF NOT EXISTS modifiers JSONB NOT NULL DEFAULT '[]';

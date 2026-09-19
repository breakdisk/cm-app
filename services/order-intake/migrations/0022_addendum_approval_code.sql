-- Migration: 0022 — approving an addendum on the lead's phone
--
-- The lead may not load what the survey added until the customer approves
-- its price. A customer on site without the app (booked on WhatsApp, say)
-- approves on the lead's phone with a one-time code sent to them with the
-- addendum. It is not the delivery PIN: typing that into the lead's phone
-- would hand the lead the code that proves the delivery.
--
-- The code is shown only to the customer; five wrong tries lock it (the
-- customer can still approve in the app).

ALTER TABLE order_intake.home_addenda
    ADD COLUMN IF NOT EXISTS approval_code     TEXT,
    ADD COLUMN IF NOT EXISTS approval_attempts INTEGER NOT NULL DEFAULT 0;

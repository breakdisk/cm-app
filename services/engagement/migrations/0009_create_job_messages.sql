-- Job chat: the thread between the customer and the driver on one shipment.
--
-- Who may read or post is not decided here. order-intake owns "this is my
-- shipment" and driver-ops owns "I am the driver on it"; engagement asks them
-- and stores the thread. The tenant is carried on every row so a read never
-- has to trust the caller for it.
CREATE TABLE IF NOT EXISTS engagement.job_messages (
    id                UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id         UUID        NOT NULL,
    shipment_id       UUID        NOT NULL,
    sender_id         UUID        NOT NULL,
    sender_role       TEXT        NOT NULL CHECK (sender_role IN ('customer', 'driver')),
    body              TEXT        NOT NULL CHECK (char_length(body) BETWEEN 1 AND 2000),
    -- The sender's own id for this message. A phone that retries a send on a
    -- flaky connection posts the same id twice and gets the same message back,
    -- instead of the customer seeing their sentence twice.
    client_message_id UUID,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT job_messages_one_per_client_id UNIQUE (sender_id, client_message_id)
);

-- The thread read: one shipment's messages in order, and the "anything since?"
-- poll both apps make while the chat is open.
CREATE INDEX IF NOT EXISTS job_messages_thread
    ON engagement.job_messages (tenant_id, shipment_id, created_at);

-- How far each person has read. One row per reader per thread.
CREATE TABLE IF NOT EXISTS engagement.job_message_reads (
    tenant_id    UUID        NOT NULL,
    shipment_id  UUID        NOT NULL,
    reader_id    UUID        NOT NULL,
    last_read_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, shipment_id, reader_id)
);

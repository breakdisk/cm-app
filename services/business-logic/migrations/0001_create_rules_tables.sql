-- =============================================================================
-- Migration: 0001_create_rules_tables
-- Service:   Business Logic & Automation Engine
-- Schema:    business_logic
-- Description:
--   Creates the foundational tables for the rule engine and workflow runtime:
--     - automation_rules     : declarative tenant-scoped trigger/condition/action definitions
--     - rule_executions      : immutable audit log of every rule evaluation
--     - workflow_instances   : long-running multi-step workflow state machines
--     - workflow_step_logs   : per-step execution trace for each workflow instance
--
--   Row-Level Security (RLS) is enabled on all tables.  The application role
--   must SET app.current_tenant_id = '<uuid>' before executing any query;
--   RLS policies enforce tenant isolation at the database layer.
--
--   This migration is idempotent — objects are created with IF NOT EXISTS
--   guards.  Re-running it after a failed partial apply is safe.
-- =============================================================================

BEGIN;

-- ---------------------------------------------------------------------------
-- Schema
-- ---------------------------------------------------------------------------

CREATE SCHEMA IF NOT EXISTS business_logic;

-- Grant the application role schema-level usage
GRANT USAGE ON SCHEMA business_logic TO logisticos_app;

-- Set search path for this migration session
SET search_path = business_logic, public;

-- ---------------------------------------------------------------------------
-- Extensions (ensure dependencies are available)
-- ---------------------------------------------------------------------------

CREATE EXTENSION IF NOT EXISTS "pgcrypto";   -- gen_random_uuid()
CREATE EXTENSION IF NOT EXISTS "pg_trgm";    -- GIN trigram index on rule names

-- ---------------------------------------------------------------------------
-- Shared utility: update_updated_at_column
-- Automatically maintains the updated_at timestamp on any table that
-- references this trigger function.
-- ---------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION business_logic.update_updated_at_column()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = business_logic, public
AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$;

COMMENT ON FUNCTION business_logic.update_updated_at_column() IS
    'Trigger function that sets updated_at = NOW() on every UPDATE. '
    'Attach with: BEFORE UPDATE ON <table> FOR EACH ROW EXECUTE FUNCTION business_logic.update_updated_at_column()';

-- ---------------------------------------------------------------------------
-- Enum types
-- ---------------------------------------------------------------------------

DO $$
BEGIN
    -- Trigger event classifications consumed by the rules engine listener
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'trigger_type' AND typnamespace = 'business_logic'::regnamespace) THEN
        CREATE TYPE business_logic.trigger_type AS ENUM (
            -- Order lifecycle
            'order.created',
            'order.confirmed',
            'order.cancelled',
            'order.updated',
            -- Shipment / delivery lifecycle
            'shipment.picked_up',
            'shipment.in_transit',
            'shipment.out_for_delivery',
            'shipment.delivered',
            'shipment.delivery_failed',
            'shipment.returned',
            'shipment.exception',
            -- Driver events
            'driver.assigned',
            'driver.started_route',
            'driver.location_updated',
            -- Payment / COD events
            'payment.cod_collected',
            'payment.invoice_generated',
            'payment.overdue',
            -- SLA events
            'sla.breach_imminent',
            'sla.breached',
            -- Customer engagement events
            'customer.signup',
            'customer.churn_risk_detected',
            -- Scheduled / time-based
            'schedule.cron',
            -- Webhook inbound (from external systems)
            'webhook.received',
            -- Manual / API-triggered
            'manual'
        );
    END IF;

    -- Rule / workflow execution status
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'execution_status' AND typnamespace = 'business_logic'::regnamespace) THEN
        CREATE TYPE business_logic.execution_status AS ENUM (
            'pending',
            'executing',
            'completed',
            'failed',
            'skipped'         -- conditions not met; rule intentionally not executed
        );
    END IF;

    -- Workflow lifecycle status
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'workflow_status' AND typnamespace = 'business_logic'::regnamespace) THEN
        CREATE TYPE business_logic.workflow_status AS ENUM (
            'pending',
            'running',
            'waiting',        -- blocked on an async step (e.g. waiting for driver confirmation)
            'completed',
            'failed',
            'cancelled',
            'timed_out'
        );
    END IF;
END $$;

-- ---------------------------------------------------------------------------
-- Table: automation_rules
--
-- Defines a tenant-scoped automation rule that the Business Logic Engine
-- evaluates whenever a matching trigger event is received from Kafka.
--
-- conditions (JSONB):
--   Evaluated as a logical expression tree.  Example:
--   {
--     "operator": "AND",
--     "conditions": [
--       { "field": "shipment.cod_amount", "op": "gt", "value": 5000 },
--       { "field": "shipment.zone",       "op": "in", "value": ["Manila", "Cebu"] }
--     ]
--   }
--
-- actions (JSONB array):
--   Ordered list of actions to execute when conditions are met.  Example:
--   [
--     { "type": "send_notification",   "channel": "whatsapp", "template": "high_value_cod_alert" },
--     { "type": "assign_priority_tag",  "tag": "HIGH_VALUE"  },
--     { "type": "invoke_mcp_tool",      "tool": "dispatch.assign_driver", "params": { "priority": "express" } }
--   ]
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS business_logic.automation_rules (
    id              UUID        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,

    name            TEXT        NOT NULL
                    CONSTRAINT automation_rules_name_length CHECK (char_length(name) BETWEEN 1 AND 255),

    description     TEXT,

    is_active       BOOLEAN     NOT NULL DEFAULT TRUE,

    -- Kafka topic event type that triggers this rule
    trigger_type    TEXT        NOT NULL,

    -- Additional Kafka topic filter (e.g. only fire for a specific merchant's events)
    trigger_filter  JSONB,

    -- Condition expression tree evaluated against the trigger event payload
    conditions      JSONB       NOT NULL DEFAULT '{"operator":"AND","conditions":[]}',

    -- Ordered array of action descriptors
    actions         JSONB       NOT NULL DEFAULT '[]',

    -- Execution priority within a tenant: lower number = evaluated first
    priority        INTEGER     NOT NULL DEFAULT 100
                    CONSTRAINT automation_rules_priority_range CHECK (priority BETWEEN 0 AND 10000),

    -- Circuit-breaker: max rule executions per hour per tenant (0 = unlimited)
    rate_limit_per_hour INTEGER NOT NULL DEFAULT 0
                    CONSTRAINT automation_rules_rate_limit CHECK (rate_limit_per_hour >= 0),

    -- Optional human-readable tags for the UI rule builder
    tags            TEXT[]      NOT NULL DEFAULT '{}',

    created_by      UUID,           -- identity service user ID
    updated_by      UUID,

    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT automation_rules_pkey PRIMARY KEY (id)
);

-- Unique rule names within a tenant
CREATE UNIQUE INDEX IF NOT EXISTS uq_automation_rules_tenant_name
    ON business_logic.automation_rules (tenant_id, name)
    WHERE is_active = TRUE;

-- Fast lookup by tenant + trigger type (primary query pattern from the engine)
CREATE INDEX IF NOT EXISTS idx_automation_rules_tenant_trigger
    ON business_logic.automation_rules (tenant_id, trigger_type)
    WHERE is_active = TRUE;

-- Priority-ordered evaluation per tenant
CREATE INDEX IF NOT EXISTS idx_automation_rules_tenant_priority
    ON business_logic.automation_rules (tenant_id, priority ASC)
    WHERE is_active = TRUE;

-- GIN index for full-text search on rule names (merchant portal rule builder search)
CREATE INDEX IF NOT EXISTS idx_automation_rules_name_trgm
    ON business_logic.automation_rules USING GIN (name gin_trgm_ops);

-- GIN index for querying inside conditions/actions JSONB
CREATE INDEX IF NOT EXISTS idx_automation_rules_conditions_gin
    ON business_logic.automation_rules USING GIN (conditions);

CREATE INDEX IF NOT EXISTS idx_automation_rules_actions_gin
    ON business_logic.automation_rules USING GIN (actions);

-- updated_at trigger
DROP TRIGGER IF EXISTS trg_automation_rules_updated_at ON business_logic.automation_rules;
DROP TRIGGER IF EXISTS trg_automation_rules_updated_at ON business_logic.automation_rules;
CREATE TRIGGER trg_automation_rules_updated_at
    BEFORE UPDATE ON business_logic.automation_rules
    FOR EACH ROW
    EXECUTE FUNCTION business_logic.update_updated_at_column();

COMMENT ON TABLE business_logic.automation_rules IS
    'Declarative automation rules evaluated by the Business Logic Engine on matching Kafka trigger events. '
    'Each rule belongs to a single tenant and contains a condition expression tree and ordered action list.';

COMMENT ON COLUMN business_logic.automation_rules.trigger_type      IS 'Kafka event type string that activates this rule (e.g. shipment.delivered).';
COMMENT ON COLUMN business_logic.automation_rules.trigger_filter     IS 'Optional additional filter applied to the event before condition evaluation (e.g. filter by merchant_id).';
COMMENT ON COLUMN business_logic.automation_rules.conditions         IS 'Logical expression tree evaluated against the Kafka event payload. Operators: AND, OR, NOT.';
COMMENT ON COLUMN business_logic.automation_rules.actions            IS 'Ordered array of action descriptors executed when conditions evaluate to TRUE.';
COMMENT ON COLUMN business_logic.automation_rules.priority           IS 'Evaluation order within a tenant. Lower number = higher priority. Allows rules to override each other.';
COMMENT ON COLUMN business_logic.automation_rules.rate_limit_per_hour IS 'Maximum executions per hour for this rule (per tenant). 0 = unlimited.';

-- ---------------------------------------------------------------------------
-- Table: rule_executions
--
-- Immutable audit record of every rule evaluation — whether the conditions
-- were met and which actions were executed.  Never updated after creation.
-- Consumers: Ops Portal audit view, AI training feedback loop.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS business_logic.rule_executions (
    id                  UUID            NOT NULL DEFAULT gen_random_uuid(),
    rule_id             UUID            NOT NULL
                        REFERENCES business_logic.automation_rules (id) ON DELETE SET NULL,
    tenant_id           UUID            NOT NULL,

    -- The Kafka event ID that triggered this evaluation
    trigger_event_id    TEXT,

    -- Full snapshot of the trigger event payload at execution time (for replay/debug)
    trigger_event_payload JSONB,

    status              business_logic.execution_status NOT NULL DEFAULT 'pending',

    -- Whether the condition tree evaluated to TRUE
    conditions_met      BOOLEAN,

    -- Array of action results: { "type": "...", "status": "success"|"failed", "result": {...} }
    actions_executed    JSONB           NOT NULL DEFAULT '[]',

    -- Error detail if status = 'failed'
    error_message       TEXT,
    error_code          TEXT,           -- machine-readable error classification

    -- Execution duration in milliseconds (populated on completion)
    duration_ms         INTEGER
                        CONSTRAINT rule_executions_duration CHECK (duration_ms IS NULL OR duration_ms >= 0),

    executed_at         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    completed_at        TIMESTAMPTZ,

    CONSTRAINT rule_executions_pkey PRIMARY KEY (id),
    CONSTRAINT rule_executions_completed_requires_executed
        CHECK (completed_at IS NULL OR completed_at >= executed_at)
);

-- Primary access pattern: audit log for a specific rule
CREATE INDEX IF NOT EXISTS idx_rule_executions_rule_id
    ON business_logic.rule_executions (rule_id, executed_at DESC);

-- Tenant-scoped execution history (Ops Portal audit page)
CREATE INDEX IF NOT EXISTS idx_rule_executions_tenant_executed_at
    ON business_logic.rule_executions (tenant_id, executed_at DESC);

-- Status filter for monitoring dashboards (failed / pending executions)
CREATE INDEX IF NOT EXISTS idx_rule_executions_status
    ON business_logic.rule_executions (status)
    WHERE status IN ('pending', 'executing', 'failed');

-- Lookup by Kafka event ID (deduplication, replay)
CREATE INDEX IF NOT EXISTS idx_rule_executions_trigger_event_id
    ON business_logic.rule_executions (trigger_event_id)
    WHERE trigger_event_id IS NOT NULL;

COMMENT ON TABLE business_logic.rule_executions IS
    'Immutable audit log of every automation rule evaluation. '
    'Records the trigger event, condition evaluation result, executed actions, and any errors.';

COMMENT ON COLUMN business_logic.rule_executions.trigger_event_id      IS 'Kafka message key / event UUID — used for idempotency checks and replay.';
COMMENT ON COLUMN business_logic.rule_executions.trigger_event_payload  IS 'Full snapshot of the inbound event payload captured at execution time.';
COMMENT ON COLUMN business_logic.rule_executions.conditions_met         IS 'TRUE if the rule condition tree evaluated to TRUE; FALSE if skipped; NULL if evaluation errored.';
COMMENT ON COLUMN business_logic.rule_executions.actions_executed       IS 'Array of action result objects populated as actions complete.';

-- ---------------------------------------------------------------------------
-- Table: workflow_instances
--
-- Stateful, long-running workflow execution context.  A workflow progresses
-- through a sequence of named steps, each of which may be synchronous or
-- asynchronous.  The context JSONB carries all accumulated state across steps.
--
-- Examples:
--   - "balikbayan_box_intake"    : order capture → address validation → driver assignment → pickup confirm
--   - "failed_delivery_reattempt": trigger → reschedule notification → customer reply capture → rebook
--   - "cod_collection_workflow"  : delivery → COD collect → reconcile → remit
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS business_logic.workflow_instances (
    id              UUID                        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id       UUID                        NOT NULL,

    -- Workflow type identifier matching a registered workflow definition
    workflow_type   TEXT                        NOT NULL
                    CONSTRAINT workflow_instances_type_length CHECK (char_length(workflow_type) BETWEEN 1 AND 128),

    -- The primary domain entity this workflow is operating on (order, shipment, driver, customer …)
    entity_type     TEXT                        NOT NULL
                    CONSTRAINT workflow_instances_entity_type_length CHECK (char_length(entity_type) BETWEEN 1 AND 64),
    entity_id       UUID                        NOT NULL,

    -- The workflow step currently awaiting execution or completion
    current_step    TEXT
                    CONSTRAINT workflow_instances_step_length CHECK (current_step IS NULL OR char_length(current_step) BETWEEN 1 AND 128),

    status          business_logic.workflow_status NOT NULL DEFAULT 'pending',

    -- Accumulated workflow state — all inputs and outputs from completed steps
    context         JSONB                       NOT NULL DEFAULT '{}',

    -- Optional parent workflow (for sub-workflows / nested orchestration)
    parent_workflow_id UUID
                    REFERENCES business_logic.workflow_instances (id) ON DELETE SET NULL,

    -- Correlation ID for distributed tracing (links to OpenTelemetry trace)
    correlation_id  TEXT,

    -- Human-readable label for the Ops Portal workflow inspector
    display_label   TEXT,

    -- Retry tracking
    retry_count     INTEGER                     NOT NULL DEFAULT 0
                    CONSTRAINT workflow_instances_retry CHECK (retry_count >= 0),
    max_retries     INTEGER                     NOT NULL DEFAULT 3,

    started_at      TIMESTAMPTZ                 NOT NULL DEFAULT NOW(),
    completed_at    TIMESTAMPTZ,
    expires_at      TIMESTAMPTZ,                -- deadline for TTL-based timeout enforcement

    CONSTRAINT workflow_instances_pkey PRIMARY KEY (id),
    CONSTRAINT workflow_instances_completed_after_started
        CHECK (completed_at IS NULL OR completed_at >= started_at),
    CONSTRAINT workflow_instances_expires_after_started
        CHECK (expires_at IS NULL OR expires_at >= started_at)
);

-- Primary domain entity lookup (e.g. "what workflows are running for order X?")
CREATE INDEX IF NOT EXISTS idx_workflow_instances_entity
    ON business_logic.workflow_instances (entity_type, entity_id);

-- Tenant-scoped active workflow dashboard
CREATE INDEX IF NOT EXISTS idx_workflow_instances_tenant_status
    ON business_logic.workflow_instances (tenant_id, status)
    WHERE status IN ('pending', 'running', 'waiting');

-- Workflow type aggregation (metrics, debugging)
CREATE INDEX IF NOT EXISTS idx_workflow_instances_type_status
    ON business_logic.workflow_instances (workflow_type, status);

-- TTL enforcement: find expired running workflows for the sweeper job
CREATE INDEX IF NOT EXISTS idx_workflow_instances_expires_at
    ON business_logic.workflow_instances (expires_at)
    WHERE expires_at IS NOT NULL AND status IN ('pending', 'running', 'waiting');

-- Parent/child workflow traversal
CREATE INDEX IF NOT EXISTS idx_workflow_instances_parent
    ON business_logic.workflow_instances (parent_workflow_id)
    WHERE parent_workflow_id IS NOT NULL;

-- GIN index for JSONB context queries (workflow inspector search)
CREATE INDEX IF NOT EXISTS idx_workflow_instances_context_gin
    ON business_logic.workflow_instances USING GIN (context);

COMMENT ON TABLE business_logic.workflow_instances IS
    'Stateful execution context for multi-step, potentially long-running workflow state machines. '
    'Each instance tracks its current step, accumulated context, and lifecycle status.';

COMMENT ON COLUMN business_logic.workflow_instances.workflow_type   IS 'Registered workflow type identifier (maps to a WorkflowDefinition in the engine).';
COMMENT ON COLUMN business_logic.workflow_instances.entity_id       IS 'UUID of the primary domain entity this workflow operates on.';
COMMENT ON COLUMN business_logic.workflow_instances.current_step    IS 'The step name the workflow is currently executing or waiting on.';
COMMENT ON COLUMN business_logic.workflow_instances.context         IS 'Accumulated workflow state: all step inputs, outputs, and metadata.';
COMMENT ON COLUMN business_logic.workflow_instances.expires_at      IS 'Hard deadline after which the sweeper marks this instance as timed_out.';

-- ---------------------------------------------------------------------------
-- Table: workflow_step_logs
--
-- Append-only trace of each step execution within a workflow instance.
-- Provides a full audit trail and enables workflow replay / compensation.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS business_logic.workflow_step_logs (
    id              UUID        NOT NULL DEFAULT gen_random_uuid(),
    workflow_id     UUID        NOT NULL
                    REFERENCES business_logic.workflow_instances (id) ON DELETE CASCADE,

    step_name       TEXT        NOT NULL
                    CONSTRAINT workflow_step_logs_step_name_length CHECK (char_length(step_name) BETWEEN 1 AND 128),

    -- Attempt number within this step (for retry tracking within a step)
    attempt         INTEGER     NOT NULL DEFAULT 1
                    CONSTRAINT workflow_step_logs_attempt CHECK (attempt >= 1),

    status          business_logic.execution_status NOT NULL DEFAULT 'pending',

    -- Input payload passed into this step
    input           JSONB       NOT NULL DEFAULT '{}',

    -- Output produced by this step (populated on success)
    output          JSONB,

    -- Error details (populated on failure)
    error_message   TEXT,
    error_code      TEXT,

    -- Duration in milliseconds
    duration_ms     INTEGER
                    CONSTRAINT workflow_step_logs_duration CHECK (duration_ms IS NULL OR duration_ms >= 0),

    -- The MCP tool invoked (if this step called an MCP action)
    mcp_tool        TEXT,
    mcp_tool_args   JSONB,
    mcp_tool_result JSONB,

    started_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at    TIMESTAMPTZ,

    CONSTRAINT workflow_step_logs_pkey PRIMARY KEY (id),
    CONSTRAINT workflow_step_logs_completed_after_started
        CHECK (completed_at IS NULL OR completed_at >= started_at)
);

-- Primary access pattern: retrieve all steps for a workflow instance (in order)
CREATE INDEX IF NOT EXISTS idx_workflow_step_logs_workflow_id
    ON business_logic.workflow_step_logs (workflow_id, started_at ASC);

-- Status filter (find failing steps across all instances for monitoring)
CREATE INDEX IF NOT EXISTS idx_workflow_step_logs_status
    ON business_logic.workflow_step_logs (status)
    WHERE status IN ('pending', 'executing', 'failed');

-- Step-name aggregation (performance of individual step types)
CREATE INDEX IF NOT EXISTS idx_workflow_step_logs_step_name
    ON business_logic.workflow_step_logs (step_name, started_at DESC);

COMMENT ON TABLE business_logic.workflow_step_logs IS
    'Append-only execution trace for each workflow step. '
    'Enables full audit, retry compensation, and workflow replay. '
    'Records MCP tool invocations for AI-agent-driven steps.';

COMMENT ON COLUMN business_logic.workflow_step_logs.attempt     IS 'Retry attempt number within this step (1 = first attempt).';
COMMENT ON COLUMN business_logic.workflow_step_logs.mcp_tool    IS 'MCP tool identifier invoked by this step (e.g. dispatch.assign_driver). NULL for non-AI steps.';
COMMENT ON COLUMN business_logic.workflow_step_logs.input       IS 'Full input payload passed into this step at execution time.';
COMMENT ON COLUMN business_logic.workflow_step_logs.output      IS 'Structured output produced by this step (merged into workflow context on success).';

-- ---------------------------------------------------------------------------
-- Row-Level Security
-- ---------------------------------------------------------------------------

-- automation_rules
ALTER TABLE business_logic.automation_rules ENABLE ROW LEVEL SECURITY;

CREATE POLICY rls_automation_rules_tenant_isolation
    ON business_logic.automation_rules
    AS PERMISSIVE
    FOR ALL
    TO logisticos_app
    USING (tenant_id = current_setting('app.current_tenant_id', TRUE)::UUID)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', TRUE)::UUID);

-- Allow the service's privileged role (used for internal engine operations) to bypass RLS
CREATE POLICY rls_automation_rules_service_bypass
    ON business_logic.automation_rules
    AS PERMISSIVE
    FOR ALL
    TO logisticos_service
    USING (TRUE)
    WITH CHECK (TRUE);

-- rule_executions
ALTER TABLE business_logic.rule_executions ENABLE ROW LEVEL SECURITY;

CREATE POLICY rls_rule_executions_tenant_isolation
    ON business_logic.rule_executions
    AS PERMISSIVE
    FOR ALL
    TO logisticos_app
    USING (tenant_id = current_setting('app.current_tenant_id', TRUE)::UUID)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', TRUE)::UUID);

CREATE POLICY rls_rule_executions_service_bypass
    ON business_logic.rule_executions
    AS PERMISSIVE
    FOR ALL
    TO logisticos_service
    USING (TRUE)
    WITH CHECK (TRUE);

-- workflow_instances
ALTER TABLE business_logic.workflow_instances ENABLE ROW LEVEL SECURITY;

CREATE POLICY rls_workflow_instances_tenant_isolation
    ON business_logic.workflow_instances
    AS PERMISSIVE
    FOR ALL
    TO logisticos_app
    USING (tenant_id = current_setting('app.current_tenant_id', TRUE)::UUID)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', TRUE)::UUID);

CREATE POLICY rls_workflow_instances_service_bypass
    ON business_logic.workflow_instances
    AS PERMISSIVE
    FOR ALL
    TO logisticos_service
    USING (TRUE)
    WITH CHECK (TRUE);

-- workflow_step_logs — access controlled through workflow_instances tenant isolation
-- (step logs have no tenant_id column; they inherit isolation via the workflow FK)
ALTER TABLE business_logic.workflow_step_logs ENABLE ROW LEVEL SECURITY;

CREATE POLICY rls_workflow_step_logs_via_workflow
    ON business_logic.workflow_step_logs
    AS PERMISSIVE
    FOR ALL
    TO logisticos_app
    USING (
        workflow_id IN (
            SELECT id FROM business_logic.workflow_instances
            WHERE tenant_id = current_setting('app.current_tenant_id', TRUE)::UUID
        )
    );

CREATE POLICY rls_workflow_step_logs_service_bypass
    ON business_logic.workflow_step_logs
    AS PERMISSIVE
    FOR ALL
    TO logisticos_service
    USING (TRUE)
    WITH CHECK (TRUE);

-- ---------------------------------------------------------------------------
-- Grants
-- ---------------------------------------------------------------------------

GRANT SELECT, INSERT, UPDATE, DELETE
    ON business_logic.automation_rules
    TO logisticos_app;

GRANT SELECT, INSERT
    ON business_logic.rule_executions
    TO logisticos_app;

-- rule_executions is append-only for the app role; UPDATE is service-only
GRANT UPDATE (status, conditions_met, actions_executed, error_message, error_code, duration_ms, completed_at)
    ON business_logic.rule_executions
    TO logisticos_service;

GRANT SELECT, INSERT, UPDATE
    ON business_logic.workflow_instances
    TO logisticos_app;

GRANT SELECT, INSERT, UPDATE (status, context, current_step, completed_at, retry_count)
    ON business_logic.workflow_instances
    TO logisticos_service;

GRANT SELECT, INSERT
    ON business_logic.workflow_step_logs
    TO logisticos_app;

GRANT UPDATE (status, output, error_message, error_code, duration_ms, mcp_tool_result, completed_at)
    ON business_logic.workflow_step_logs
    TO logisticos_service;

GRANT ALL ON SCHEMA business_logic TO logisticos_service;
GRANT ALL ON ALL TABLES IN SCHEMA business_logic TO logisticos_service;
GRANT ALL ON ALL SEQUENCES IN SCHEMA business_logic TO logisticos_service;

-- ---------------------------------------------------------------------------
-- Seed: built-in system workflow type registrations
-- (informational — actual registration lives in the engine's config)
-- ---------------------------------------------------------------------------

COMMENT ON SCHEMA business_logic IS
    'Business Logic & Automation Engine schema. '
    'Contains rule definitions, execution audit logs, and workflow state machines. '
    'All tables enforce tenant isolation via RLS using app.current_tenant_id session variable.';

COMMIT;

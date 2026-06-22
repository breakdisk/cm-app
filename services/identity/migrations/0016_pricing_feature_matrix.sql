-- Platform-level feature matrix. Maps feature keys to the tiers that include
-- them. Admins update `enabled_tiers` to control which pricing categories unlock
-- each capability. Not tenant-scoped — one row per feature, global config.
CREATE TABLE IF NOT EXISTS identity.pricing_features (
    feature_key      TEXT        PRIMARY KEY,
    feature_name     TEXT        NOT NULL,
    feature_category TEXT        NOT NULL CHECK (feature_category IN ('logistics','ai','engagement','platform')),
    description      TEXT,
    enabled_tiers    TEXT[]      NOT NULL DEFAULT '{}',
    is_system        BOOLEAN     NOT NULL DEFAULT FALSE,
    sort_order       INT         NOT NULL DEFAULT 0,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Seed with the features previously hardcoded in the admin portal frontend.
-- enabled_tiers mirrors libs/types/src/lib.rs::SubscriptionTier capability methods.
INSERT INTO identity.pricing_features
    (feature_key, feature_name, feature_category, description, enabled_tiers, is_system, sort_order)
VALUES
    ('real_time_tracking',  'Real-Time Driver Tracking',  'logistics',   'Live GPS tracking for active driver routes', ARRAY['starter','growth','business','enterprise'], TRUE,  10),
    ('cod_reconciliation',  'COD Auto-Reconciliation',    'logistics',   'Automatic cash-on-delivery settlement with driver ledger', ARRAY['starter','growth','business','enterprise'], TRUE, 20),
    ('balikbayan_service',  'Balikbayan Box Service',     'logistics',   'Oversized freight booking and POP/POD workflow', ARRAY['starter','growth','business','enterprise'], TRUE, 30),
    ('ai_dispatch',         'AI Dispatch Agent',          'ai',          'ML-driven driver assignment and route optimisation', ARRAY['growth','business','enterprise'], FALSE, 40),
    ('same_day_delivery',   'Same-Day Delivery',          'logistics',   'Sub-24h delivery SLA with dynamic route recalculation', ARRAY['growth','business','enterprise'], FALSE, 50),
    ('ai_recovery_agent',   'AI Recovery Agent',          'ai',          'Autonomous failed-delivery rescheduling and customer outreach', ARRAY['business','enterprise'], FALSE, 60),
    ('loyalty_program',     'Loyalty Program',            'engagement',  'Points-based rewards and repeat-booking incentives', ARRAY['business','enterprise'], FALSE, 70),
    ('dynamic_pricing',     'Dynamic Pricing',            'platform',    'Surge and zone-based rate adjustments in real time', ARRAY['business','enterprise'], FALSE, 80),
    ('enterprise_mcp',      'Enterprise MCP Extension',   'platform',    'Register external MCP servers and build custom AI workflows on platform data', ARRAY['enterprise'], FALSE, 90),
    ('white_label',         'White-Label Branding',       'platform',    'Custom domain, logo, colours, and legal text on all customer-facing surfaces', ARRAY['enterprise'], FALSE, 100)
ON CONFLICT (feature_key) DO NOTHING;

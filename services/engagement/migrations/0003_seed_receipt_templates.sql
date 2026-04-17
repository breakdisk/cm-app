-- Migration: 0003 — Seed receipt notification templates for MVP flow
-- These platform-level templates (tenant_id IS NULL) are used by the event_consumer
-- when processing shipment.created, pickup.completed, and delivery.completed events.

-- Shipment confirmation receipt — WhatsApp
INSERT INTO engagement.notification_templates
    (template_id, channel, language, subject, body, variables)
VALUES
    ('shipment_confirmation', 'email', 'en',
     'Shipment Confirmed — {{tracking_number}}',
     '<h2>Shipment Confirmed</h2>
<p>Hi {{customer_name}},</p>
<p>Your shipment has been confirmed and is being processed.</p>
<table style="width:100%;border-collapse:collapse;margin:16px 0;">
  <tr><td style="padding:8px;border-bottom:1px solid #eee;font-weight:600;">Tracking Number</td><td style="padding:8px;border-bottom:1px solid #eee;">{{tracking_number}}</td></tr>
  <tr><td style="padding:8px;border-bottom:1px solid #eee;font-weight:600;">From</td><td style="padding:8px;border-bottom:1px solid #eee;">{{origin_address}}</td></tr>
  <tr><td style="padding:8px;border-bottom:1px solid #eee;font-weight:600;">To</td><td style="padding:8px;border-bottom:1px solid #eee;">{{destination_address}}</td></tr>
  <tr><td style="padding:8px;border-bottom:1px solid #eee;font-weight:600;">Service</td><td style="padding:8px;border-bottom:1px solid #eee;">{{service_type}}</td></tr>
  <tr><td style="padding:8px;border-bottom:1px solid #eee;font-weight:600;">Weight</td><td style="padding:8px;border-bottom:1px solid #eee;">{{weight}}</td></tr>
  <tr><td style="padding:8px;border-bottom:1px solid #eee;font-weight:600;">Fee</td><td style="padding:8px;border-bottom:1px solid #eee;">{{total_fee}}</td></tr>
  <tr><td style="padding:8px;border-bottom:1px solid #eee;font-weight:600;">COD</td><td style="padding:8px;border-bottom:1px solid #eee;">{{cod_amount}}</td></tr>
</table>
<p><a href="{{tracking_url}}" style="display:inline-block;padding:12px 24px;background:#00E5FF;color:#000;text-decoration:none;border-radius:8px;font-weight:600;">Track Your Shipment</a></p>
<p style="color:#666;font-size:12px;">Thank you for choosing CargoMarket!</p>',
     ARRAY['customer_name','tracking_number','origin_address','destination_address','service_type','weight','total_fee','cod_amount','tracking_url'])
ON CONFLICT (tenant_id, template_id, channel, language) DO UPDATE
    SET body = EXCLUDED.body, subject = EXCLUDED.subject, variables = EXCLUDED.variables;

-- Shipment picked up — WhatsApp
INSERT INTO engagement.notification_templates
    (template_id, channel, language, subject, body, variables)
VALUES
    ('shipment_picked_up', 'whatsapp', 'en', NULL,
     'Hi {{customer_name}}! Your package {{tracking_number}} has been picked up by our rider and is on its way. Track it: {{tracking_url}}',
     ARRAY['customer_name','tracking_number','tracking_url'])
ON CONFLICT (tenant_id, template_id, channel, language) DO UPDATE
    SET body = EXCLUDED.body, variables = EXCLUDED.variables;

-- Delivery confirmed receipt — Email (rich HTML)
INSERT INTO engagement.notification_templates
    (template_id, channel, language, subject, body, variables)
VALUES
    ('delivery_confirmed', 'email', 'en',
     'Delivery Confirmed — {{tracking_number}}',
     '<h2>Delivery Confirmed</h2>
<p>Hi {{customer_name}},</p>
<p>Your shipment <strong>{{tracking_number}}</strong> has been successfully delivered.</p>
<table style="width:100%;border-collapse:collapse;margin:16px 0;">
  <tr><td style="padding:8px;border-bottom:1px solid #eee;font-weight:600;">Delivered At</td><td style="padding:8px;border-bottom:1px solid #eee;">{{completed_at}}</td></tr>
  <tr><td style="padding:8px;border-bottom:1px solid #eee;font-weight:600;">Tracking</td><td style="padding:8px;border-bottom:1px solid #eee;">{{tracking_number}}</td></tr>
</table>
<p><a href="{{tracking_url}}" style="display:inline-block;padding:12px 24px;background:#00FF88;color:#000;text-decoration:none;border-radius:8px;font-weight:600;">View Delivery Details</a></p>
<p style="color:#666;font-size:12px;">Thank you for shipping with CargoMarket!</p>',
     ARRAY['customer_name','tracking_number','completed_at','tracking_url'])
ON CONFLICT (tenant_id, template_id, channel, language) DO UPDATE
    SET body = EXCLUDED.body, subject = EXCLUDED.subject, variables = EXCLUDED.variables;

-- COD receipt — WhatsApp
INSERT INTO engagement.notification_templates
    (template_id, channel, language, subject, body, variables)
VALUES
    ('cod_receipt', 'whatsapp', 'en', NULL,
     'Cash on Delivery collected for {{tracking_number}}. Amount: {{cod_amount}}. Thank you! — CargoMarket',
     ARRAY['tracking_number','cod_amount'])
ON CONFLICT (tenant_id, template_id, channel, language) DO UPDATE
    SET body = EXCLUDED.body, variables = EXCLUDED.variables;

/**
 * Invoices / Payment Receipts API
 *
 * Talks to the payments service.
 * GET /v1/customers/:customerId/invoices  — list receipts for the logged-in customer
 * GET /v1/invoices/:id                    — single receipt detail
 */
import { createApiClient } from './client';

let cachedPaymentsClient: ReturnType<typeof createApiClient> | null = null;

function getPaymentsClient() {
  if (!cachedPaymentsClient) {
    cachedPaymentsClient = createApiClient(
      process.env.EXPO_PUBLIC_PAYMENTS_URL ||
      process.env.EXPO_PUBLIC_API_URL ||
      'http://localhost:8012'
    );
  }
  return cachedPaymentsClient;
}

// ── List summary shape (returned by GET /v1/customers/:id/invoices) ───────────
// Field names match the payments service InvoiceSummary DTO (updated 2026-06).

export interface InvoiceSummary {
  id:             string;
  invoice_number: string;
  invoice_type:   string;   // "payment_receipt" | "shipment_charges" | etc.
  status:         string;   // "paid" | "issued" | "draft" | ...
  awb_count:      number;
  subtotal_php:   number;
  vat_php:        number;
  total_php:      number;
  period_from:    string;   // ISO date, e.g. "2026-06-15"
  period_to:      string;   // ISO date, e.g. "2026-06-15"
  due_date:       string;   // RFC 3339
  paid_at:        string | null;
  created_at:     string;   // RFC 3339 (= issued_at)
}

// ── Single invoice detail shape (returned by GET /v1/invoices/:id) ────────────
// The full domain Invoice entity — contains line items with real fee breakdown.

export interface InvoiceDetail {
  id:             string;
  invoice_number: string;
  invoice_type:   string;
  status:         string;
  currency:       string;
  issued_at:      string;
  due_at:         string;
  paid_at:        string | null;
  line_items:     Array<{
    charge_type:  string;
    description:  string;
    quantity:     number;
    unit_price:   { amount: number; currency: string };
    discount:     { amount: number; currency: string } | null;
  }>;
  total_due:      { amount: number; currency: string };
}

export async function listCustomerInvoices(customerId: string): Promise<InvoiceSummary[]> {
  const client = getPaymentsClient();
  const res = await client.get<{ data: InvoiceSummary[] }>(
    `/v1/customers/${customerId}/invoices`
  );
  return res.data.data ?? [];
}

export async function getInvoice(invoiceId: string): Promise<InvoiceDetail> {
  const client = getPaymentsClient();
  const res = await client.get<{ data: InvoiceDetail }>(`/v1/invoices/${invoiceId}`);
  return res.data.data;
}

/** Re-send a payment receipt to the customer's email on file. */
export async function resendInvoice(invoiceId: string): Promise<{ sent: boolean }> {
  const client = getPaymentsClient();
  const res = await client.post<{ data: { sent: boolean } }>(
    `/v1/invoices/${invoiceId}/resend`,
    {}
  );
  return res.data.data ?? { sent: true };
}

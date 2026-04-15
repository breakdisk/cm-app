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

// Shape returned by the payments service for each receipt in the list
export interface InvoiceSummary {
  invoice_id:     string;
  invoice_number: string;
  invoice_type:   string;   // "payment_receipt"
  status:         string;   // "paid" | "issued" | ...
  awb_count:      number;
  subtotal_cents: number;
  vat_cents:      number;
  total_cents:    number;
  billing_period: string;   // "YYYY-MM"
  due_at:         string;   // ISO8601
  issued_at:      string;   // ISO8601
}

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
    charge_type:     string;
    description:     string;
    quantity:        number;
    unit_price:      { amount: number; currency: string };
    discount:        { amount: number; currency: string } | null;
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

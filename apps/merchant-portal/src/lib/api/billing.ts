import { createApiClient, ApiResponse, PaginatedApiResponse } from "./client";

export interface Invoice {
  id: string;
  invoice_number: string;
  tenant_id: string;
  merchant_id?: string;
  status: InvoiceStatus;
  period_from: string;
  period_to: string;
  line_items: InvoiceLineItem[];
  subtotal_php: number;
  tax_php: number;
  total_php: number;
  paid_at?: string;
  due_date: string;
  created_at: string;
}

export type InvoiceStatus =
  | "draft"
  | "issued"
  | "paid"
  | "overdue"
  | "cancelled";

export interface InvoiceLineItem {
  description: string;
  quantity: number;
  unit_price_php: number;
  total_php: number;
}

export interface CodBalance {
  merchant_id: string;
  merchant_name: string;
  pending_php: number;
  remitted_php: number;
  last_remittance_at?: string;
  next_remittance_date?: string;
  shipments_pending_cod: number;
}

export interface CodRemittance {
  id: string;
  merchant_id: string;
  amount_php: number;
  shipment_count: number;
  status: "pending" | "processing" | "completed" | "failed";
  bank_reference?: string;
  created_at: string;
  completed_at?: string;
}

export interface Wallet {
  tenant_id: string;
  balance_php: number;
  reserved_php: number;
  available_php: number;
  currency: "PHP";
}

export interface WalletTransaction {
  id: string;
  type: "credit" | "debit";
  amount_php: number;
  description: string;
  reference_id?: string;
  balance_after_php: number;
  created_at: string;
}

export const billingApi = {
  // ── Invoices ──────────────────────────────────────────────────

  /** List invoices for the tenant */
  listInvoices: (
    params: {
      status?: InvoiceStatus;
      merchant_id?: string;
      page?: number;
      per_page?: number;
    },
    token: string
  ) =>
    createApiClient(token)
      .get<PaginatedApiResponse<Invoice>>("/v1/billing/invoices", { params })
      .then((r) => r.data),

  /** Get a single invoice */
  getInvoice: (invoiceId: string, token: string) =>
    createApiClient(token)
      .get<ApiResponse<Invoice>>(`/v1/billing/invoices/${invoiceId}`)
      .then((r) => r.data.data),

  /** Generate invoice for a billing period */
  generateInvoice: (
    payload: {
      merchant_id?: string;
      period_from: string;
      period_to: string;
    },
    token: string
  ) =>
    createApiClient(token)
      .post<ApiResponse<Invoice>>("/v1/billing/invoices/generate", payload)
      .then((r) => r.data.data),

  /** Download invoice as PDF */
  downloadInvoicePdf: (invoiceId: string, token: string) =>
    createApiClient(token)
      .get<Blob>(`/v1/billing/invoices/${invoiceId}/pdf`, {
        responseType: "blob",
      })
      .then((r) => r.data),

  // ── COD Management ────────────────────────────────────────────

  /** Get COD balance for a merchant */
  getCodBalance: (merchantId: string, token: string) =>
    createApiClient(token)
      .get<ApiResponse<CodBalance>>(`/v1/billing/cod/balance/${merchantId}`)
      .then((r) => r.data.data),

  /** List COD remittances */
  listCodRemittances: (
    params: {
      merchant_id?: string;
      status?: string;
      page?: number;
      per_page?: number;
    },
    token: string
  ) =>
    createApiClient(token)
      .get<PaginatedApiResponse<CodRemittance>>("/v1/billing/cod/remittances", {
        params,
      })
      .then((r) => r.data),

  /** Initiate COD reconciliation for a merchant */
  reconcileCod: (merchantId: string, token: string) =>
    createApiClient(token)
      .post<ApiResponse<CodRemittance>>(
        `/v1/billing/cod/reconcile/${merchantId}`
      )
      .then((r) => r.data.data),

  // ── Wallet ────────────────────────────────────────────────────

  /** Get tenant wallet balance */
  getWallet: (token: string) =>
    createApiClient(token)
      .get<ApiResponse<Wallet>>("/v1/billing/wallet")
      .then((r) => r.data.data),

  /** Get wallet transaction history */
  getWalletTransactions: (
    params: { cursor?: string; limit?: number },
    token: string
  ) =>
    createApiClient(token)
      .get<{ data: WalletTransaction[]; next_cursor: string | null }>(
        "/v1/billing/wallet/transactions",
        { params }
      )
      .then((r) => r.data),
};

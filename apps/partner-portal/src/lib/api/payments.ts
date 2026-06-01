import { createApiClient } from "./client";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface Wallet {
  wallet_id:     string;
  tenant_id:     string;
  balance_php:   number;
  reserved_php:  number;
  available_php: number;
  currency:      "PHP";
  updated_at:    string;
}

export interface WalletTransaction {
  id:           string;
  type:         "credit" | "debit";
  amount_php:   number;
  description:  string;
  reference_id: string;
  created_at:   string;
}

export type InvoiceStatus =
  | "draft"
  | "issued"
  | "paid"
  | "overdue"
  | "cancelled"
  | "disputed";

export interface Invoice {
  id:             string;
  invoice_number: string;
  invoice_type:   string;
  status:         InvoiceStatus;
  awb_count:      number;
  subtotal_php:   number;
  vat_php:        number;
  total_php:      number;
  period_from:    string;
  period_to:      string;
  due_date:       string;
  paid_at?:       string | null;
  created_at:     string;
}

export type WithdrawalStatus = "pending" | "approved" | "disbursed" | "rejected";

export interface WithdrawalHistoryItem {
  id:          string;
  amount_php:  number;
  currency:    string;
  status:      WithdrawalStatus;
  review_note: string | null;
  created_at:  string;
  updated_at:  string;
}

// ── API ───────────────────────────────────────────────────────────────────────

export const paymentsApi = {
  /** Get the carrier wallet balance (available, reserved, total). */
  async getWallet(): Promise<Wallet> {
    const { data } = await createApiClient().get<{ data: Wallet }>("/v1/wallet");
    return data.data;
  },

  /** List recent wallet transactions, newest first. Default limit: 20. */
  async getTransactions(limit = 20): Promise<WalletTransaction[]> {
    const { data } = await createApiClient().get<{ data: WalletTransaction[] }>(
      "/v1/wallet/transactions",
      { params: { limit } }
    );
    return data.data ?? [];
  },

  /** List invoices for the authenticated carrier's tenant. */
  async getInvoices(): Promise<Invoice[]> {
    const { data } = await createApiClient().get<{ data: Invoice[] }>("/v1/invoices");
    return data.data ?? [];
  },

  /**
   * Request a withdrawal from the carrier wallet.
   * Sends amount in centavos (amount_php × 100) to match the backend command.
   * Optionally sends carrier_email so the engagement service can email the
   * partner when the withdrawal is disbursed or rejected.
   * Returns the updated wallet summary.
   */
  async withdraw(amount_php: number, carrier_email?: string): Promise<Wallet> {
    const { data } = await createApiClient().post<{ data: Wallet }>(
      "/v1/wallet/withdraw",
      {
        amount_cents:  Math.round(amount_php * 100),
        carrier_email: carrier_email ?? null,
      }
    );
    return data.data;
  },

  /** List all withdrawal requests for the authenticated carrier, newest first. */
  async getWithdrawalRequests(): Promise<WithdrawalHistoryItem[]> {
    const { data } = await createApiClient().get<{ data: WithdrawalHistoryItem[] }>(
      "/v1/wallet/withdrawal-requests"
    );
    return data.data ?? [];
  },
};

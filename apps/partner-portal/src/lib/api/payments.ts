import { createApiClient } from "./client";

// ── Types ─────────────────────────────────────────────────────────────────────

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
  reference_id?: string | null;
  balance_after_php: number;
  created_at: string;
}

export type InvoiceStatus = "draft" | "issued" | "paid" | "overdue" | "cancelled";

export interface Invoice {
  id: string;
  invoice_number: string;
  status: InvoiceStatus;
  period_from: string;
  period_to: string;
  total_php: number;
  due_date: string;
  paid_at?: string | null;
  created_at: string;
}

export interface WithdrawRequest {
  amount_php: number;
}

// ── API ───────────────────────────────────────────────────────────────────────

export const paymentsApi = {
  async getWallet(): Promise<Wallet> {
    const { data } = await createApiClient().get<{ data: Wallet }>("/v1/wallet");
    return data.data;
  },

  async getTransactions(limit = 20): Promise<WalletTransaction[]> {
    const { data } = await createApiClient().get<{ data: WalletTransaction[] }>(
      "/v1/wallet/transactions",
      { params: { limit } }
    );
    return data.data ?? [];
  },

  async getInvoices(): Promise<Invoice[]> {
    const { data } = await createApiClient().get<{ data: Invoice[] }>("/v1/invoices");
    return data.data ?? [];
  },

  async withdraw(amount_php: number): Promise<Wallet> {
    const { data } = await createApiClient().post<{ data: Wallet }>(
      "/v1/wallet/withdraw",
      { amount_php }
    );
    return data.data;
  },
};

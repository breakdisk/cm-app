import { createApiClient } from './client';
import type { AxiosInstance } from 'axios';

let cachedPaymentsClient: AxiosInstance | null = null;

function getPaymentsClient(): AxiosInstance {
  if (!cachedPaymentsClient) {
    cachedPaymentsClient = createApiClient(
      process.env.EXPO_PUBLIC_PAYMENTS_URL || process.env.EXPO_PUBLIC_API_URL || 'http://localhost:8012'
    );
  }
  return cachedPaymentsClient;
}

export interface WalletData {
  wallet_id: string;
  balance_cents: number;
  available_cents: number;
  reserved_cents: number;
  currency: string;
}

export interface WalletTransaction {
  id: string;
  type: 'credit' | 'debit';
  amount_cents: number;
  description: string;
  reference_id?: string | null;
  balance_after_cents: number;
  created_at: string;
}

export interface DeliveryReceipt {
  awb: string;
  status: string;
  serviceType: string;
  origin: string;
  destination: string;
  recipientName: string;
  createdAt: string;
  deliveredAt?: string;
  eta?: string;
  totalFee: number;
  currency: string;
  isCod: boolean;
  codAmount?: number;
  codCollected?: boolean;
  podId?: string;
}

export const paymentsApi = {
  getWallet: () => {
    return getPaymentsClient().get<{ data: WalletData }>('/v1/wallet');
  },

  getTransactions: (limit = 20) => {
    return getPaymentsClient().get<{ data: WalletTransaction[] }>(
      '/v1/wallet/transactions',
      { params: { limit } }
    );
  },

  withdraw: (amount_cents: number) => {
    return getPaymentsClient().post<{ data: WalletData }>(
      '/v1/wallet/withdraw',
      { amount_cents }
    );
  },
};

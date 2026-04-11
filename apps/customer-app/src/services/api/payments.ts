import { createApiClient, ApiError } from './client';
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
  /** Get wallet balance (for merchant use — customer-facing receipts use tracking data) */
  getWallet: () => {
    const client = getPaymentsClient();
    return client.get<{ data: { wallet_id: string; balance_cents: number; currency: string } }>('/v1/wallet');
  },
};

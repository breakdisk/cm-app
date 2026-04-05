import { getIdentityClient, ApiError } from './client';

export interface CustomerProfile {
  id: string;
  name: string;
  phone: string;
  email: string;
  kycStatus: 'pending' | 'submitted' | 'verified' | 'rejected';
  loyaltyPoints: number;
  createdAt: string;
}

export interface UpdateCustomerRequest {
  name?: string;
  email?: string;
}

export async function getCustomer(customerId: string): Promise<CustomerProfile> {
  try {
    const identityClient = getIdentityClient();
    const response = await identityClient.get<CustomerProfile>(`/v1/customers/${customerId}`);
    return response.data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

export async function updateCustomer(customerId: string, request: UpdateCustomerRequest): Promise<CustomerProfile> {
  try {
    const identityClient = getIdentityClient();
    const response = await identityClient.put<CustomerProfile>(`/v1/customers/${customerId}`, request);
    return response.data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

export async function submitKYC(customerId: string, documents: any): Promise<{ status: string }> {
  try {
    const identityClient = getIdentityClient();
    const response = await identityClient.post(`/v1/customers/${customerId}/kyc`, documents);
    return response.data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

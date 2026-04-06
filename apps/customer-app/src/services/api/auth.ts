import * as SecureStore from 'expo-secure-store';
import { getIdentityClient, ApiError } from './client';

export interface VerifyPhoneRequest {
  phone: string;
}

export interface VerifyOTPRequest {
  phone: string;
  otp: string;
}

export interface AuthResponse {
  token: string;
  refreshToken: string;
  customerId: string;
  name: string;
  email: string;
}

export async function verifyPhone(phone: string): Promise<void> {
  try {
    const client = getIdentityClient();
    await client.post<void>('/v1/auth/verify-phone', { phone });
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

export async function verifyOTP(phone: string, otp: string): Promise<AuthResponse> {
  try {
    const client = getIdentityClient();
    const response = await client.post<AuthResponse>('/v1/auth/verify-otp', {
      phone,
      otp,
    });

    const { token, refreshToken, customerId, name, email } = response.data;

    // Store tokens securely
    await SecureStore.setItemAsync('auth_token', token);
    await SecureStore.setItemAsync('refresh_token', refreshToken);
    await SecureStore.setItemAsync('customer_id', customerId);

    return { token, refreshToken, customerId, name, email };
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

export async function logout(): Promise<void> {
  try {
    const client = getIdentityClient();
    await client.post('/v1/auth/logout');
  } finally {
    await SecureStore.deleteItemAsync('auth_token');
    await SecureStore.deleteItemAsync('refresh_token');
    await SecureStore.deleteItemAsync('customer_id');
  }
}

export async function getStoredToken(): Promise<string | null> {
  try {
    return await SecureStore.getItemAsync('auth_token');
  } catch (error) {
    console.error('Failed to retrieve token:', error);
    return null;
  }
}

export async function getStoredCustomerId(): Promise<string | null> {
  try {
    return await SecureStore.getItemAsync('customer_id');
  } catch (error) {
    console.error('Failed to retrieve customer ID:', error);
    return null;
  }
}

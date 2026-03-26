/**
 * Secure token storage for the driver app.
 * Uses expo-secure-store (Keychain on iOS, Keystore on Android).
 */
import * as SecureStore from "expo-secure-store";

const ACCESS_TOKEN_KEY  = "driver_access_token";
const REFRESH_TOKEN_KEY = "driver_refresh_token";

export const tokenStore = {
  async setTokens(access: string, refresh: string): Promise<void> {
    await Promise.all([
      SecureStore.setItemAsync(ACCESS_TOKEN_KEY,  access),
      SecureStore.setItemAsync(REFRESH_TOKEN_KEY, refresh),
    ]);
  },

  async getAccessToken(): Promise<string | null> {
    return SecureStore.getItemAsync(ACCESS_TOKEN_KEY);
  },

  async getRefreshToken(): Promise<string | null> {
    return SecureStore.getItemAsync(REFRESH_TOKEN_KEY);
  },

  async clearTokens(): Promise<void> {
    await Promise.all([
      SecureStore.deleteItemAsync(ACCESS_TOKEN_KEY),
      SecureStore.deleteItemAsync(REFRESH_TOKEN_KEY),
    ]);
  },
};

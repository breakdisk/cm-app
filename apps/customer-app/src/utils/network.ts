/**
 * Network detection utilities for offline/online status monitoring
 */
import { useEffect, useState } from 'react';
import { useNetInfo } from '@react-native-community/netinfo';

/**
 * Hook to detect current network connection status
 * Returns boolean indicating whether device is connected to network
 */
export function useOnlineStatus(): boolean {
  const netInfo = useNetInfo();
  return netInfo.isConnected ?? false;
}

/**
 * Async function to check network status by making a HEAD request
 * Useful for validating actual connectivity (not just network availability)
 */
export async function checkNetworkStatus(): Promise<boolean> {
  try {
    const response = await fetch('https://www.google.com', { method: 'HEAD' });
    return response.ok;
  } catch {
    return false;
  }
}

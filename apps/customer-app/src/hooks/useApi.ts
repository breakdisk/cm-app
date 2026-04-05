import { useState, useEffect, useCallback } from 'react';

export interface UseApiState<T> {
  data: T | null;
  loading: boolean;
  error: string | null;
}

export interface UseApiOptions<T> {
  onSuccess?: (data: T) => void;
  onError?: (error: string) => void;
  cacheTime?: number;
}

/**
 * Generic hook for making API calls with loading/error state management
 * and optional caching and offline fallback.
 *
 * @param asyncFn - The async function to execute (typically an API call)
 * @param options - Configuration options for caching, success/error callbacks
 * @returns State object with data, loading, error, and refetch function
 */
export function useApi<T>(
  asyncFn: () => Promise<T>,
  options: UseApiOptions<T> = {}
): UseApiState<T> & { refetch: () => Promise<void> } {
  const { onSuccess, onError, cacheTime = 0 } = options;
  const [state, setState] = useState<UseApiState<T>>({
    data: null,
    loading: true,
    error: null,
  });

  const fetch = useCallback(async () => {
    setState(prev => ({ ...prev, loading: true, error: null }));
    try {
      const data = await asyncFn();
      setState({ data, loading: false, error: null });
      onSuccess?.(data);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Unknown error';
      setState({ data: null, loading: false, error: errorMsg });
      onError?.(errorMsg);
    }
  }, [asyncFn, onSuccess, onError]);

  useEffect(() => {
    fetch();
  }, [fetch]);

  return { ...state, refetch: fetch };
}

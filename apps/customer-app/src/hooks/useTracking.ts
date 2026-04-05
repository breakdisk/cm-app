import { useEffect, useRef, useCallback } from 'react';
import { useAppDispatch, useAppSelector } from '../store/hooks';
import {
  setTrackingLoading,
  setTrackingData,
  setTrackingError,
  addToHistory,
} from '../store/slices/tracking';
import * as trackingService from '../services/api/tracking';

export interface UseTrackingOptions {
  pollInterval?: number;
  autoload?: boolean;
}

export interface UseTrackingResult {
  data: any | null;
  loading: boolean;
  error: string | null;
  refetch: () => Promise<void>;
}

/**
 * Hook for real-time tracking with polling subscription.
 * Maintains polling subscription and cleans up on unmount.
 * Dispatches tracking updates to Redux state.
 *
 * @param awb - The Air Way Bill tracking number
 * @param options - Configuration for polling interval and autoload
 * @returns Tracking data, loading/error states, and manual refetch function
 */
export function useTracking(
  awb: string,
  options: UseTrackingOptions = {}
): UseTrackingResult {
  const { pollInterval = 30000, autoload = true } = options;
  const dispatch = useAppDispatch();
  const unsubscribeRef = useRef<(() => void) | null>(null);
  const trackingState = useAppSelector(state => state.tracking);

  const data = trackingState.byAwb[awb] || null;
  const loading = trackingState.loading[awb] || false;
  const error = trackingState.error[awb] || null;

  const refetch = useCallback(async () => {
    dispatch(setTrackingLoading({ awb, loading: true }));
    try {
      const trackingData = await trackingService.getTracking(awb);
      dispatch(setTrackingData(trackingData as any));
      dispatch(setTrackingError({ awb, error: null }));
    } catch (err) {
      dispatch(
        setTrackingError({
          awb,
          error: err instanceof Error ? err.message : 'Failed to load tracking',
        })
      );
    } finally {
      dispatch(setTrackingLoading({ awb, loading: false }));
    }
  }, [awb, dispatch]);

  useEffect(() => {
    if (!autoload) {
      return;
    }

    dispatch(setTrackingLoading({ awb, loading: true }));

    const subscribe = async () => {
      try {
        unsubscribeRef.current = await trackingService.subscribeToTrackingUpdates(
          awb,
          trackingData => {
            dispatch(setTrackingData(trackingData as any));
            dispatch(setTrackingError({ awb, error: null }));
            dispatch(addToHistory(trackingData as any));
          }
        );
      } catch (err) {
        dispatch(
          setTrackingError({
            awb,
            error: err instanceof Error ? err.message : 'Failed to load tracking',
          })
        );
      } finally {
        dispatch(setTrackingLoading({ awb, loading: false }));
      }
    };

    subscribe();

    return () => {
      if (unsubscribeRef.current) {
        unsubscribeRef.current();
      }
    };
  }, [awb, autoload, dispatch]);

  return { data, loading, error, refetch };
}

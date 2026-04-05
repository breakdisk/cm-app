import { useEffect, useCallback } from 'react';
import { useAppDispatch, useAppSelector } from '../store/hooks';
import { setLoading, setShipments, setError, Shipment } from '../store/slices/shipments';
import * as shipmentsService from '../services/api/shipments';
import { getStoredCustomerId } from '../services/api/auth';

export interface UseShipmentsOptions {
  skip?: number;
  limit?: number;
  status?: string;
  autoload?: boolean;
}

/**
 * Hook to load and manage shipments list with Redux state management.
 * Automatically fetches shipments on mount using stored customer ID.
 *
 * @param options - Configuration options for pagination and filtering
 * @returns Redux state with shipments list, loading, error, pagination, and refetch function
 */
export function useShipments(options: UseShipmentsOptions = {}) {
  const { skip = 0, limit = 20, status, autoload = true } = options;
  const dispatch = useAppDispatch();
  const state = useAppSelector(state => state.shipments);

  const loadShipments = useCallback(async () => {
    dispatch(setLoading(true));
    try {
      const customerId = await getStoredCustomerId();
      if (!customerId) {
        dispatch(setError('Not authenticated'));
        return;
      }

      const response = await shipmentsService.listShipments(customerId, {
        skip,
        limit,
        status,
      });

      // Convert API response to Shipment type with default values
      const shipments: Shipment[] = response.shipments.map((ship: any) => ({
        awb: ship.awb,
        status: ship.status as any,
        origin: ship.origin,
        destination: ship.destination,
        date: ship.createdAt,
        fee: ship.fee,
        totalFee: ship.fee,
        currency: ship.currency as 'PHP' | 'USD',
        type: 'local',
      }));

      dispatch(
        setShipments({
          shipments,
          total: response.total,
        })
      );
      dispatch(setError(null));
    } catch (err) {
      dispatch(setError(err instanceof Error ? err.message : 'Failed to load shipments'));
    } finally {
      dispatch(setLoading(false));
    }
  }, [dispatch, skip, limit, status]);

  useEffect(() => {
    if (autoload) {
      loadShipments();
    }
  }, [autoload, loadShipments]);

  return { ...state, refetch: loadShipments };
}

/**
 * Hook to get a single shipment by AWB with caching.
 * Looks up cached shipment in Redux store; fetches if not cached.
 *
 * @param awb - The Air Way Bill tracking number
 * @returns Cached shipment data or null if not found
 */
export function useShipmentById(awb: string) {
  const byAwb = useAppSelector(state => state.shipments.byAwb);
  const cached = byAwb[awb];

  useEffect(() => {
    if (!cached) {
      const loadShipment = async () => {
        try {
          await shipmentsService.getShipment(awb);
        } catch (err) {
          console.error('Failed to load shipment:', err);
        }
      };
      loadShipment();
    }
  }, [awb, cached]);

  return cached || null;
}

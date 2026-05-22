import { useEffect, useCallback } from 'react';
import { useAppDispatch, useAppSelector } from '../store/hooks';
import { setLoading, setShipments, setError, Shipment } from '../store/slices/shipments';
import * as shipmentsService from '../services/api/shipments';

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
      const response = await shipmentsService.listShipments({ skip, limit, status });

      // Map API response (order-intake shape) to Redux Shipment type
      const shipments: Shipment[] = response.shipments.map((ship: any) => {
        // Determine shipment type: balikbayan is always international; otherwise
        // check if either endpoint is outside PH.
        const isIntl =
          ship.service_type === 'balikbayan' ||
          (ship.destination?.country_code && ship.destination.country_code !== 'PH') ||
          (ship.origin?.country_code && ship.origin.country_code !== 'PH');

        const originStr = typeof ship.origin === 'object'
          ? `${ship.origin.line1 ?? ''}, ${ship.origin.city ?? ''}`.replace(/^,\s*/, '').trim()
          : (ship.origin ?? '');

        const destinationStr = typeof ship.destination === 'object'
          ? `${ship.destination.line1 ?? ''}, ${ship.destination.city ?? ''}`.replace(/^,\s*/, '').trim()
          : (ship.destination ?? '');

        const codCents = ship.cod_amount_cents ?? 0;

        return {
          id:           ship.id,
          awb:          ship.awb ?? ship.tracking_number,
          status:       ship.status as any,
          origin:       originStr,
          destination:  destinationStr,
          date:         ship.created_at ?? ship.createdAt,
          bookedAt:     ship.created_at ?? ship.createdAt,
          // Freight fee: order-intake list doesn't expose the rated freight amount
          // yet (it's computed by the payments service). Use 0 until the field lands.
          fee:          0,
          totalFee:     0,
          currency:     'PHP' as const,
          type:         (isIntl ? 'international' : 'local') as any,
          freightMode:  ship.service_type === 'balikbayan' ? 'sea' : undefined,
          isCOD:        codCents > 0,
          codAmount:    codCents > 0 ? String(codCents / 100) : undefined,
          estimatedDelivery: ship.estimated_delivery,
          description:  ship.description,
          weight:       ship.weight_grams != null ? String(ship.weight_grams / 1000) : undefined,
          recipientName:  ship.customer_name,
          recipientPhone: ship.customer_phone,
          destCountry:  (!isIntl || ship.destination?.country_code === 'PH')
            ? undefined
            : ship.destination?.country_code,
        };
      });

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

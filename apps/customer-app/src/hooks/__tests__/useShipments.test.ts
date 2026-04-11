import React from 'react';
import { renderHook, act, waitFor } from '@testing-library/react-native';
import { Provider } from 'react-redux';
import { store } from '../../store';
import { useShipments, useShipmentById } from '../useShipments';
import * as shipmentsService from '../../services/api/shipments';

// Mock the API services
jest.mock('../../services/api/shipments');

const mockShipmentsService = shipmentsService as jest.Mocked<typeof shipmentsService>;

const wrapper = ({ children }: { children: React.ReactNode }) =>
  React.createElement(Provider, { store, children });

const emptyListResponse = { shipments: [], total: 0 };

const mockAddress = {
  line1: '123 Main St', city: 'Manila', province: 'NCR',
  postal_code: '1000', country_code: 'PH',
};

describe('useShipments', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  test('calls API service on mount', async () => {
    mockShipmentsService.listShipments.mockResolvedValue(emptyListResponse);

    renderHook(() => useShipments(), { wrapper });

    await waitFor(() => {
      expect(mockShipmentsService.listShipments).toHaveBeenCalledWith({
        skip: 0, limit: 20, status: undefined,
      });
    });
  });

  test('handles API error gracefully', async () => {
    mockShipmentsService.listShipments.mockRejectedValue(new Error('Network error'));

    renderHook(() => useShipments(), { wrapper });

    await waitFor(() => {
      expect(mockShipmentsService.listShipments).toHaveBeenCalled();
    });
  });

  test('supports pagination options', async () => {
    mockShipmentsService.listShipments.mockResolvedValue(emptyListResponse);

    renderHook(() => useShipments({ skip: 20, limit: 10 }), { wrapper });

    await waitFor(() => {
      expect(mockShipmentsService.listShipments).toHaveBeenCalledWith({
        skip: 20, limit: 10, status: undefined,
      });
    });
  });

  test('refetch calls API again', async () => {
    mockShipmentsService.listShipments.mockResolvedValue(emptyListResponse);

    const { result } = renderHook(() => useShipments(), { wrapper });

    await waitFor(() => {
      expect(mockShipmentsService.listShipments).toHaveBeenCalledTimes(1);
    });

    await act(async () => {
      await result.current.refetch();
    });

    expect(mockShipmentsService.listShipments).toHaveBeenCalledTimes(2);
  });

  test('respects autoload: false', async () => {
    mockShipmentsService.listShipments.mockResolvedValue(emptyListResponse);

    const { result } = renderHook(() => useShipments({ autoload: false }), { wrapper });

    expect(mockShipmentsService.listShipments).not.toHaveBeenCalled();

    await act(async () => {
      await result.current.refetch();
    });

    expect(mockShipmentsService.listShipments).toHaveBeenCalled();
  });
});

describe('useShipmentById', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  test('returns null for non-cached shipment', () => {
    const { result } = renderHook(() => useShipmentById('AWB-NOT-CACHED'), { wrapper });
    expect(result.current).toBeNull();
  });

  test('attempts to fetch shipment on mount', async () => {
    mockShipmentsService.getShipment.mockResolvedValue({
      id: 'id-002', awb: 'AWB002', tracking_number: 'AWB002',
      status: 'in_transit', service_type: 'standard', origin: mockAddress, destination: mockAddress,
      customer_name: 'Test', customer_phone: '+63912', weight_grams: 500,
      created_at: '2024-01-01',
    });

    renderHook(() => useShipmentById('AWB002'), { wrapper });

    await waitFor(() => {
      expect(mockShipmentsService.getShipment).toHaveBeenCalledWith('AWB002');
    });
  });

  test('does not fetch if shipment is already cached', async () => {
    mockShipmentsService.getShipment.mockResolvedValue({
      id: 'id-003', awb: 'AWB003', tracking_number: 'AWB003',
      status: 'delivered', service_type: 'standard', origin: mockAddress, destination: mockAddress,
      customer_name: 'Test', customer_phone: '+63912', weight_grams: 500,
      created_at: '2024-01-01',
    });

    const { rerender } = renderHook(
      ({ awb }: { awb: string }) => useShipmentById(awb),
      { wrapper, initialProps: { awb: 'AWB003' } }
    );

    await waitFor(() => {
      expect(mockShipmentsService.getShipment).toHaveBeenCalledWith('AWB003');
    });

    mockShipmentsService.getShipment.mockClear();
    rerender({ awb: 'AWB003' });
    expect(mockShipmentsService.getShipment).not.toHaveBeenCalled();
  });
});

import React from 'react';
import { renderHook, act, waitFor } from '@testing-library/react-native';
import { Provider } from 'react-redux';
import { store } from '../../store';
import { useShipments, useShipmentById } from '../useShipments';
import * as shipmentsService from '../../services/api/shipments';
import * as authService from '../../services/api/auth';

// Mock the API services
jest.mock('../../services/api/shipments');
jest.mock('../../services/api/auth');

const mockShipmentsService = shipmentsService as jest.Mocked<typeof shipmentsService>;
const mockAuthService = authService as jest.Mocked<typeof authService>;

const wrapper = ({ children }: { children: React.ReactNode }) =>
  React.createElement(Provider, { store, children });

describe('useShipments', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockAuthService.getStoredCustomerId.mockResolvedValue('customer-123');
  });

  test('calls API service on mount with customer ID', async () => {
    mockShipmentsService.listShipments.mockResolvedValue({
      shipments: [],
      total: 0,
      skip: 0,
      limit: 20,
    });

    renderHook(() => useShipments(), { wrapper });

    await waitFor(() => {
      expect(mockShipmentsService.listShipments).toHaveBeenCalledWith('customer-123', {
        skip: 0,
        limit: 20,
        status: undefined,
      });
    });
  });

  test('handles authentication error', async () => {
    mockAuthService.getStoredCustomerId.mockResolvedValue(null);
    mockShipmentsService.listShipments.mockResolvedValue({
      shipments: [],
      total: 0,
      skip: 0,
      limit: 20,
    });

    renderHook(() => useShipments(), { wrapper });

    // Should not call API if no customer ID
    await waitFor(() => {
      expect(mockShipmentsService.listShipments).not.toHaveBeenCalled();
    });
  });

  test('handles API error gracefully', async () => {
    mockShipmentsService.listShipments.mockRejectedValue(new Error('Network error'));

    const { result } = renderHook(() => useShipments(), { wrapper });

    // Should attempt to call API
    await waitFor(() => {
      expect(mockShipmentsService.listShipments).toHaveBeenCalled();
    });
  });

  test('supports pagination options', async () => {
    mockShipmentsService.listShipments.mockResolvedValue({
      shipments: [],
      total: 0,
      skip: 20,
      limit: 10,
    });

    renderHook(() => useShipments({ skip: 20, limit: 10 }), { wrapper });

    await waitFor(() => {
      expect(mockShipmentsService.listShipments).toHaveBeenCalledWith('customer-123', {
        skip: 20,
        limit: 10,
        status: undefined,
      });
    });
  });

  test('refetch function calls API again', async () => {
    mockShipmentsService.listShipments.mockResolvedValue({
      shipments: [],
      total: 0,
      skip: 0,
      limit: 20,
    });

    const { result } = renderHook(() => useShipments(), { wrapper });

    // Initial call
    await waitFor(() => {
      expect(mockShipmentsService.listShipments).toHaveBeenCalledTimes(1);
    });

    // Call refetch
    await act(async () => {
      await result.current.refetch();
    });

    // Verify API was called again
    expect(mockShipmentsService.listShipments).toHaveBeenCalledTimes(2);
  });

  test('respects autoload option', async () => {
    mockShipmentsService.listShipments.mockResolvedValue({
      shipments: [],
      total: 0,
      skip: 0,
      limit: 20,
    });

    const { result } = renderHook(() => useShipments({ autoload: false }), { wrapper });

    // API should not be called on mount
    expect(mockShipmentsService.listShipments).not.toHaveBeenCalled();

    // But refetch should work
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
      awb: 'AWB002',
      status: 'in_transit',
      origin: 'New York',
      destination: 'Los Angeles',
      createdAt: '2024-01-01',
      fee: 50,
      currency: 'USD',
    });

    renderHook(() => useShipmentById('AWB002'), { wrapper });

    await waitFor(() => {
      expect(mockShipmentsService.getShipment).toHaveBeenCalledWith('AWB002');
    });
  });

  test('does not fetch if shipment is already cached', async () => {
    mockShipmentsService.getShipment.mockResolvedValue({
      awb: 'AWB003',
      status: 'delivered',
      origin: 'New York',
      destination: 'Los Angeles',
      createdAt: '2024-01-01',
      fee: 50,
      currency: 'USD',
    });

    // First call fetches the shipment
    const { rerender } = renderHook(
      ({ awb }: { awb: string }) => useShipmentById(awb),
      { wrapper, initialProps: { awb: 'AWB003' } }
    );

    await waitFor(() => {
      expect(mockShipmentsService.getShipment).toHaveBeenCalledWith('AWB003');
    });

    mockShipmentsService.getShipment.mockClear();

    // Second call with same AWB should use cache
    rerender({ awb: 'AWB003' });

    // Should not call API again
    expect(mockShipmentsService.getShipment).not.toHaveBeenCalled();
  });
});

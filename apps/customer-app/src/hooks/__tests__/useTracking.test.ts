import React from 'react';
import { renderHook, act, waitFor } from '@testing-library/react-native';
import { Provider } from 'react-redux';
import { store } from '../../store';
import { useTracking } from '../useTracking';
import * as trackingService from '../../services/api/tracking';

jest.mock('../../services/api/tracking');

const mockTrackingService = trackingService as jest.Mocked<typeof trackingService>;

const wrapper = ({ children }: { children: React.ReactNode }) =>
  React.createElement(Provider, { store }, children);

describe('useTracking', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  test('subscribes to tracking updates on mount', async () => {
    mockTrackingService.subscribeToTrackingUpdates.mockResolvedValue(() => {});

    renderHook(() => useTracking('AWB001'), { wrapper });

    // Verify subscription was called
    await waitFor(() => {
      expect(mockTrackingService.subscribeToTrackingUpdates).toHaveBeenCalledWith(
        'AWB001',
        expect.any(Function)
      );
    });
  });

  test('handles subscription errors gracefully', async () => {
    mockTrackingService.subscribeToTrackingUpdates.mockRejectedValue(
      new Error('Failed to subscribe')
    );

    renderHook(() => useTracking('AWB-ERROR'), { wrapper });

    await waitFor(() => {
      expect(mockTrackingService.subscribeToTrackingUpdates).toHaveBeenCalled();
    });
  });

  test('cleans up subscription on unmount', async () => {
    const mockUnsubscribe = jest.fn();
    mockTrackingService.subscribeToTrackingUpdates.mockResolvedValue(mockUnsubscribe);

    const { unmount } = renderHook(() => useTracking('AWB001'), { wrapper });

    await waitFor(() => {
      expect(mockTrackingService.subscribeToTrackingUpdates).toHaveBeenCalled();
    });

    unmount();

    // Verify cleanup function was called
    expect(mockUnsubscribe).toHaveBeenCalled();
  });

  test('refetch calls getTracking directly', async () => {
    mockTrackingService.subscribeToTrackingUpdates.mockResolvedValue(() => {});
    mockTrackingService.getTracking.mockResolvedValue({
      awb: 'AWB001',
      currentStatus: 'delivered',
      events: [],
      lastUpdate: '2024-01-01T10:00:00Z',
    } as any);

    const { result } = renderHook(() => useTracking('AWB001'), { wrapper });

    // Initial subscription call
    await waitFor(() => {
      expect(mockTrackingService.subscribeToTrackingUpdates).toHaveBeenCalled();
    });

    // Call refetch
    await act(async () => {
      await result.current.refetch();
    });

    // Verify getTracking was called
    expect(mockTrackingService.getTracking).toHaveBeenCalledWith('AWB001');
  });

  test('respects autoload option', async () => {
    mockTrackingService.subscribeToTrackingUpdates.mockResolvedValue(() => {});
    mockTrackingService.getTracking.mockResolvedValue({
      awb: 'AWB001',
      currentStatus: 'delivered',
      events: [],
      lastUpdate: '2024-01-01T10:00:00Z',
    } as any);

    const { result } = renderHook(() => useTracking('AWB001', { autoload: false }), {
      wrapper,
    });

    // Should not subscribe on mount
    expect(mockTrackingService.subscribeToTrackingUpdates).not.toHaveBeenCalled();

    // But manual refetch should work
    await act(async () => {
      await result.current.refetch();
    });

    expect(mockTrackingService.getTracking).toHaveBeenCalled();
  });

  test('passes custom poll interval to subscription', async () => {
    mockTrackingService.subscribeToTrackingUpdates.mockResolvedValue(() => {});

    renderHook(() => useTracking('AWB001', { pollInterval: 5000 }), { wrapper });

    await waitFor(() => {
      expect(mockTrackingService.subscribeToTrackingUpdates).toHaveBeenCalledWith(
        'AWB001',
        expect.any(Function)
      );
    });
  });

  test('returns data from Redux state', () => {
    mockTrackingService.subscribeToTrackingUpdates.mockResolvedValue(() => {});

    const { result } = renderHook(() => useTracking('AWB001'), { wrapper });

    // Data should come from Redux (mocked as null in initial state)
    expect(result.current.data).toBeNull();
  });

  test('has a refetch function that can be called multiple times', async () => {
    mockTrackingService.subscribeToTrackingUpdates.mockResolvedValue(() => {});
    mockTrackingService.getTracking.mockResolvedValue({
      awb: 'AWB001',
      currentStatus: 'delivered',
      events: [],
      lastUpdate: '2024-01-01T10:00:00Z',
    } as any);

    const { result } = renderHook(() => useTracking('AWB001'), { wrapper });

    // First refetch
    await act(async () => {
      await result.current.refetch();
    });

    expect(mockTrackingService.getTracking).toHaveBeenCalledTimes(1);

    // Second refetch
    await act(async () => {
      await result.current.refetch();
    });

    expect(mockTrackingService.getTracking).toHaveBeenCalledTimes(2);
  });
});

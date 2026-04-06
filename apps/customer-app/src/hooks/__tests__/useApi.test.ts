import { renderHook, act, waitFor } from '@testing-library/react-native';
import { useApi } from '../useApi';

describe('useApi', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  test('loads data successfully on mount', async () => {
    const mockAsyncFn = jest.fn().mockResolvedValue({ id: 1, name: 'Test' });

    const { result } = renderHook(() => useApi(mockAsyncFn));

    // Initially loading
    expect(result.current.loading).toBe(true);
    expect(result.current.data).toBeNull();
    expect(result.current.error).toBeNull();

    // Wait for data to load
    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.data).toEqual({ id: 1, name: 'Test' });
    expect(result.current.error).toBeNull();
    expect(mockAsyncFn).toHaveBeenCalledTimes(1);
  });

  test('handles errors gracefully', async () => {
    const mockError = new Error('API request failed');
    const mockAsyncFn = jest.fn().mockRejectedValue(mockError);

    const { result } = renderHook(() => useApi(mockAsyncFn));

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.data).toBeNull();
    expect(result.current.error).toBe('API request failed');
  });

  test('handles non-Error exceptions', async () => {
    const mockAsyncFn = jest.fn().mockRejectedValue('Unknown error string');

    const { result } = renderHook(() => useApi(mockAsyncFn));

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.error).toBe('Unknown error');
  });

  test('calls onSuccess callback on successful load', async () => {
    const mockData = { id: 1, name: 'Test' };
    const mockAsyncFn = jest.fn().mockResolvedValue(mockData);
    const onSuccess = jest.fn();

    renderHook(() => useApi(mockAsyncFn, { onSuccess }));

    await waitFor(() => {
      expect(onSuccess).toHaveBeenCalled();
    });

    expect(onSuccess).toHaveBeenCalledWith(mockData);
  });

  test('calls onError callback on error', async () => {
    const mockError = new Error('Request failed');
    const mockAsyncFn = jest.fn().mockRejectedValue(mockError);
    const onError = jest.fn();

    renderHook(() => useApi(mockAsyncFn, { onError }));

    await waitFor(() => {
      expect(onError).toHaveBeenCalled();
    });

    expect(onError).toHaveBeenCalledWith('Request failed');
  });

  test('refetch function works correctly', async () => {
    const mockAsyncFn = jest
      .fn()
      .mockResolvedValueOnce({ id: 1 })
      .mockResolvedValueOnce({ id: 2 });

    const { result } = renderHook(() => useApi(mockAsyncFn));

    // Initial load
    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.data).toEqual({ id: 1 });
    expect(mockAsyncFn).toHaveBeenCalledTimes(1);

    // Refetch
    await act(async () => {
      await result.current.refetch();
    });

    expect(mockAsyncFn).toHaveBeenCalledTimes(2);
    expect(result.current.data).toEqual({ id: 2 });
  });

  test('refetch sets loading state correctly', async () => {
    let resolvePromise: (value: any) => void;
    const mockPromise = new Promise(resolve => {
      resolvePromise = resolve;
    });
    const mockAsyncFn = jest.fn().mockReturnValue(mockPromise);

    const { result } = renderHook(() => useApi(mockAsyncFn));

    // Initial load
    await waitFor(() => {
      expect(result.current.loading).toBe(true);
    });

    await act(async () => {
      resolvePromise!({ id: 1 });
      await mockPromise;
    });

    // Now refetch
    let refetchPromise: Promise<void>;
    let refetchResolve: (value: any) => void;
    const refetchMock = new Promise(resolve => {
      refetchResolve = resolve;
    });

    const mockAsyncFn2 = jest.fn().mockReturnValue(refetchMock);

    const { result: result2 } = renderHook(() => useApi(mockAsyncFn2));

    await waitFor(() => {
      expect(result2.current.loading).toBe(true);
    });

    await act(async () => {
      refetchResolve!({ id: 2 });
      await refetchMock;
    });

    expect(result2.current.loading).toBe(false);
  });

  test('clears error on successful refetch', async () => {
    const mockAsyncFn = jest
      .fn()
      .mockRejectedValueOnce(new Error('First error'))
      .mockResolvedValueOnce({ id: 1 });

    const { result } = renderHook(() => useApi(mockAsyncFn));

    // Initial error
    await waitFor(() => {
      expect(result.current.error).toBe('First error');
    });

    // Refetch with success
    await act(async () => {
      await result.current.refetch();
    });

    expect(result.current.error).toBeNull();
    expect(result.current.data).toEqual({ id: 1 });
  });

  test('handles dependency changes correctly', async () => {
    const mockAsyncFn1 = jest.fn().mockResolvedValue({ id: 1 });
    const mockAsyncFn2 = jest.fn().mockResolvedValue({ id: 2 });

    const { result, rerender } = renderHook(
      ({ fn }: { fn: () => Promise<any> }) => useApi(fn),
      {
        initialProps: { fn: mockAsyncFn1 },
      }
    );

    // First render
    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(mockAsyncFn1).toHaveBeenCalledTimes(1);

    // Rerender with new function
    rerender({ fn: mockAsyncFn2 });

    // Should call the new function
    await waitFor(() => {
      expect(mockAsyncFn2).toHaveBeenCalled();
    });
  });

  test('supports generic types correctly', async () => {
    interface ApiResponse {
      userId: string;
      userName: string;
    }

    const mockAsyncFn = jest.fn().mockResolvedValue({
      userId: '123',
      userName: 'John Doe',
    });

    const { result } = renderHook(() => useApi<ApiResponse>(mockAsyncFn));

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.data?.userId).toBe('123');
    expect(result.current.data?.userName).toBe('John Doe');
  });
});

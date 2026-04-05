jest.mock('axios');
jest.mock('expo-secure-store', () => ({
  getItemAsync: jest.fn(() => Promise.resolve(null)),
  setItemAsync: jest.fn(() => Promise.resolve()),
  deleteItemAsync: jest.fn(() => Promise.resolve()),
}));

import axios from 'axios';
import { createApiClient } from '../client';

const mockedAxios = axios as jest.Mocked<typeof axios>;

describe('API Client', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    // Setup axios.create mock to return a proper mock client
    mockedAxios.create.mockReturnValue({
      defaults: {
        baseURL: 'http://localhost:8001',
        headers: { common: {} },
      },
      interceptors: {
        request: { use: jest.fn((success, error) => jest.fn()) },
        response: { use: jest.fn((success, error) => jest.fn()) },
      },
      post: jest.fn(),
      get: jest.fn(),
    } as any);
  });

  test('creates axios instance with correct base URL', () => {
    createApiClient('http://localhost:8001');
    expect(mockedAxios.create).toHaveBeenCalledWith(
      expect.objectContaining({
        baseURL: 'http://localhost:8001',
        timeout: 30000,
      })
    );
  });

  test('request interceptor is registered', () => {
    const mockUse = jest.fn();
    mockedAxios.create.mockReturnValue({
      defaults: {
        baseURL: 'http://localhost:8001',
        headers: { common: {} },
      },
      interceptors: {
        request: { use: mockUse },
        response: { use: jest.fn() },
      },
    } as any);

    createApiClient('http://localhost:8001');
    expect(mockUse).toHaveBeenCalled();
  });

  test('response interceptor is registered', () => {
    const mockUse = jest.fn();
    mockedAxios.create.mockReturnValue({
      defaults: {
        baseURL: 'http://localhost:8001',
        headers: { common: {} },
      },
      interceptors: {
        request: { use: jest.fn() },
        response: { use: mockUse },
      },
    } as any);

    createApiClient('http://localhost:8001');
    expect(mockUse).toHaveBeenCalled();
  });

  test('includes Content-Type header', () => {
    createApiClient('http://localhost:8001');
    expect(mockedAxios.create).toHaveBeenCalledWith(
      expect.objectContaining({
        headers: expect.objectContaining({
          'Content-Type': 'application/json',
        }),
      })
    );
  });
});

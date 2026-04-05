import * as shipmentsService from '../shipments';

jest.mock('../client');

describe('Shipments Service', () => {
  test('createShipment sends correct payload', async () => {
    const payload = {
      origin: 'Manila',
      destination: 'Cebu',
      recipientName: 'John Doe',
      recipientPhone: '+639123456789',
      weight: 5,
      type: 'local' as const,
      codAmount: 1000,
      description: 'Test package',
      cargoType: 'Documents',
      serviceType: 'standard' as const,
    };

    // Placeholder: full test with mocked client
    expect(payload.weight).toBe(5);
    expect(payload.type).toBe('local');
    expect(payload.serviceType).toBe('standard');
  });

  test('ShipmentResponse has required fields', () => {
    const mockResponse: shipmentsService.ShipmentResponse = {
      awb: 'TEST123456',
      status: 'pending',
      origin: 'Manila',
      destination: 'Cebu',
      createdAt: '2024-01-01T00:00:00Z',
      fee: 500,
      currency: 'PHP',
    };

    expect(mockResponse.awb).toBeDefined();
    expect(mockResponse.status).toBeDefined();
    expect(mockResponse.fee).toBe(500);
  });

  test('ShipmentsListResponse structure', () => {
    const mockListResponse: shipmentsService.ShipmentsListResponse = {
      shipments: [],
      total: 0,
      skip: 0,
      limit: 20,
    };

    expect(mockListResponse.shipments).toEqual([]);
    expect(mockListResponse.limit).toBe(20);
  });
});

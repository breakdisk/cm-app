import * as shipmentsService from '../shipments';

jest.mock('../client');

const mockAddress: shipmentsService.AddressInput = {
  line1: '123 Main St',
  city: 'Manila',
  province: 'NCR',
  postal_code: '1000',
  country_code: 'PH',
};

describe('Shipments Service', () => {
  test('createShipment payload shape is valid', () => {
    const payload: shipmentsService.CreateShipmentRequest = {
      customer_name: 'John Doe',
      customer_phone: '+639123456789',
      origin: mockAddress,
      destination: { ...mockAddress, city: 'Cebu' },
      service_type: 'standard',
      weight_grams: 5000,
      cod_amount_cents: 100000,
      description: 'Test package',
    };

    expect(payload.weight_grams).toBe(5000);
    expect(payload.service_type).toBe('standard');
    expect(payload.cod_amount_cents).toBe(100000);
  });

  test('ShipmentResponse has required fields', () => {
    const mockResponse: shipmentsService.ShipmentResponse = {
      id: 'uuid-001',
      awb: 'TEST123456',
      tracking_number: 'TEST123456',
      status: 'pending',
      service_type: 'standard',
      origin: mockAddress,
      destination: mockAddress,
      customer_name: 'John Doe',
      customer_phone: '+639123456789',
      weight_grams: 5000,
      created_at: '2024-01-01T00:00:00Z',
    };

    expect(mockResponse.awb).toBeDefined();
    expect(mockResponse.status).toBeDefined();
    expect(mockResponse.weight_grams).toBe(5000);
  });

  test('ShipmentsListResponse structure', () => {
    const mockListResponse: shipmentsService.ShipmentsListResponse = {
      shipments: [],
      total: 0,
    };

    expect(mockListResponse.shipments).toEqual([]);
    expect(mockListResponse.total).toBe(0);
  });
});

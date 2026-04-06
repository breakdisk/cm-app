import { savePendingShipment, getOfflineShipments, getShipmentByAwb, getSyncMetadata } from '../sync';

// Mock the API calls and database
jest.mock('../../services/api/shipments');
jest.mock('../../services/api/auth');

describe('Database Sync Module', () => {
  describe('savePendingShipment', () => {
    test('returns a valid shipment ID with pending prefix', async () => {
      const customerId = 'test-customer-001';
      const shipmentData = {
        origin: 'Manila',
        destination: 'Cebu',
        fee: 150,
        currency: 'PHP',
        type: 'local',
        recipientName: 'John Doe',
        recipientPhone: '+639123456789',
      };

      // Mock the database
      const mockDb = {
        runAsync: jest.fn().mockResolvedValue(undefined),
      };

      jest.spyOn(require('../sqlite'), 'getDatabase').mockResolvedValue(mockDb);

      const shipmentId = await savePendingShipment(customerId, shipmentData);

      expect(shipmentId).toBeDefined();
      expect(typeof shipmentId).toBe('string');
      expect(shipmentId).toContain('pending-');
      expect(mockDb.runAsync).toHaveBeenCalled();
    });
  });

  describe('getOfflineShipments', () => {
    test('returns empty array on database error', async () => {
      const customerId = 'test-customer-error';

      const mockDb = {
        getAllAsync: jest.fn().mockRejectedValue(new Error('DB error')),
      };

      jest.spyOn(require('../sqlite'), 'getDatabase').mockResolvedValue(mockDb);

      const shipments = await getOfflineShipments(customerId);

      expect(Array.isArray(shipments)).toBe(true);
      expect(shipments.length).toBe(0);
    });

    test('handles null database response gracefully', async () => {
      const customerId = 'test-customer-no-results';

      const mockDb = {
        getAllAsync: jest.fn().mockResolvedValue(null),
      };

      jest.spyOn(require('../sqlite'), 'getDatabase').mockResolvedValue(mockDb);

      const shipments = await getOfflineShipments(customerId);

      expect(Array.isArray(shipments)).toBe(true);
      expect(shipments.length).toBe(0);
    });
  });

  describe('getShipmentByAwb', () => {
    test('returns null for database error', async () => {
      const mockDb = {
        getFirstAsync: jest.fn().mockRejectedValue(new Error('DB error')),
      };

      jest.spyOn(require('../sqlite'), 'getDatabase').mockResolvedValue(mockDb);

      const shipment = await getShipmentByAwb('TEST-AWB', 'test-customer');

      expect(shipment).toBeNull();
    });

    test('returns null when database returns null', async () => {
      const mockDb = {
        getFirstAsync: jest.fn().mockResolvedValue(null),
      };

      jest.spyOn(require('../sqlite'), 'getDatabase').mockResolvedValue(mockDb);

      const shipment = await getShipmentByAwb('NON-EXISTENT-AWB', 'test-customer');

      expect(shipment).toBeNull();
    });

    test('returns shipment when found in database', async () => {
      const mockShipment = {
        id: 'test-id',
        awb: 'TEST-AWB-123',
        customerId: 'customer-1',
        origin: 'Manila',
        destination: 'Cebu',
        status: 'pending',
      };

      const mockDb = {
        getFirstAsync: jest.fn().mockResolvedValue(mockShipment),
      };

      jest.spyOn(require('../sqlite'), 'getDatabase').mockResolvedValue(mockDb);

      const shipment = await getShipmentByAwb('TEST-AWB-123', 'customer-1');

      expect(shipment).toBeDefined();
      expect(shipment.awb).toBe('TEST-AWB-123');
      expect(shipment.origin).toBe('Manila');
    });
  });

  describe('getSyncMetadata', () => {
    test('returns null when no sync metadata exists', async () => {
      const mockDb = {
        getFirstAsync: jest.fn().mockResolvedValue(null),
      };

      jest.spyOn(require('../sqlite'), 'getDatabase').mockResolvedValue(mockDb);

      const metadata = await getSyncMetadata('shipments');

      expect(metadata).toBeNull();
    });

    test('returns metadata when it exists', async () => {
      const mockMetadata = {
        resource: 'shipments',
        lastSyncedAt: '2024-01-01T00:00:00Z',
        syncStatus: 'success',
      };

      const mockDb = {
        getFirstAsync: jest.fn().mockResolvedValue(mockMetadata),
      };

      jest.spyOn(require('../sqlite'), 'getDatabase').mockResolvedValue(mockDb);

      const metadata = await getSyncMetadata('shipments');

      expect(metadata).toBeDefined();
      expect(metadata.resource).toBe('shipments');
      expect(metadata.syncStatus).toBe('success');
    });

    test('handles database errors gracefully', async () => {
      const mockDb = {
        getFirstAsync: jest.fn().mockRejectedValue(new Error('DB error')),
      };

      jest.spyOn(require('../sqlite'), 'getDatabase').mockResolvedValue(mockDb);

      const metadata = await getSyncMetadata('shipments');

      expect(metadata).toBeNull();
    });
  });
});

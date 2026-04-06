import { getDatabase } from './sqlite';
import * as shipmentsService from '../services/api/shipments';
import { getStoredCustomerId } from '../services/api/auth';

/**
 * Sync pending shipments with the server
 * - Upload locally created shipments (isPending=1)
 * - Download latest shipments from API
 * - Update sync metadata
 */
export async function syncShipments(): Promise<void> {
  const db = await getDatabase();
  const customerId = await getStoredCustomerId();

  if (!customerId) {
    console.warn('No customer ID found, skipping sync');
    return;
  }

  try {
    // 1. Upload pending shipments created offline
    const pending = await db.getAllAsync<any>(
      `SELECT * FROM shipments WHERE customerId = ? AND isPending = 1`,
      [customerId]
    );

    for (const shipment of pending) {
      try {
        const result = await shipmentsService.createShipment(customerId, {
          origin: shipment.origin,
          destination: shipment.destination,
          recipientName: shipment.recipientName,
          recipientPhone: shipment.recipientPhone,
          weight: 0,
          description: '',
          cargoType: 'goods',
          type: shipment.type,
          serviceType: 'standard',
          codAmount: shipment.codAmount,
        });

        // Mark as synced with server-generated AWB
        await db.runAsync(
          `UPDATE shipments SET isPending = 0, syncedAt = ?, awb = ? WHERE id = ?`,
          [new Date().toISOString(), result.awb, shipment.id]
        );

        console.log(`Synced pending shipment: ${shipment.id}`);
      } catch (error) {
        console.error(`Failed to sync pending shipment ${shipment.awb}:`, error);
      }
    }

    // 2. Download latest shipments from API
    const response = await shipmentsService.listShipments(customerId, { limit: 100 });

    // Clear synced shipments and re-populate (keep pending ones)
    await db.runAsync(
      `DELETE FROM shipments WHERE customerId = ? AND isPending = 0`,
      [customerId]
    );

    for (const shipment of response.shipments) {
      await db.runAsync(
        `INSERT OR REPLACE INTO shipments
         (id, awb, customerId, origin, destination, status, fee, currency, type, recipientName, recipientPhone, createdAt, syncedAt, isPending)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)`,
        [
          `${shipment.awb}-${Date.now()}`,
          shipment.awb,
          customerId,
          shipment.origin,
          shipment.destination,
          shipment.status,
          shipment.fee,
          shipment.currency,
          'local',
          shipment.origin || 'unknown',
          shipment.destination || '',
          shipment.createdAt,
          new Date().toISOString(),
        ]
      );
    }

    // 3. Update sync metadata
    await db.runAsync(
      `INSERT OR REPLACE INTO synced_metadata (resource, lastSyncedAt, syncStatus)
       VALUES (?, ?, ?)`,
      ['shipments', new Date().toISOString(), 'success']
    );

    console.log('Shipments sync completed successfully');
  } catch (error) {
    console.error('Sync failed:', error);

    // Record sync failure in metadata
    await db.runAsync(
      `INSERT OR REPLACE INTO synced_metadata (resource, lastSyncedAt, syncStatus)
       VALUES (?, ?, ?)`,
      ['shipments', new Date().toISOString(), 'failed']
    );

    throw error;
  }
}

/**
 * Save a shipment that was created while offline
 */
export async function savePendingShipment(customerId: string, shipmentData: any): Promise<string> {
  const db = await getDatabase();

  const shipmentId = `pending-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  const temporaryAwb = `PENDING-${Math.random().toString(36).substr(2, 8).toUpperCase()}`;

  await db.runAsync(
    `INSERT INTO shipments
     (id, awb, customerId, origin, destination, status, fee, currency, type, recipientName, recipientPhone, createdAt, isPending)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)`,
    [
      shipmentId,
      temporaryAwb,
      customerId,
      shipmentData.origin,
      shipmentData.destination,
      'pending',
      shipmentData.fee || 0,
      shipmentData.currency || 'PHP',
      shipmentData.type || 'local',
      shipmentData.recipientName,
      shipmentData.recipientPhone,
      new Date().toISOString(),
    ]
  );

  return shipmentId;
}

/**
 * Get all offline shipments (both pending and synced) for a customer
 */
export async function getOfflineShipments(customerId: string): Promise<any[]> {
  const db = await getDatabase();

  try {
    const shipments = await db.getAllAsync(
      `SELECT * FROM shipments WHERE customerId = ? ORDER BY createdAt DESC`,
      [customerId]
    );
    return shipments || [];
  } catch (error) {
    console.error('Failed to fetch offline shipments:', error);
    return [];
  }
}

/**
 * Get a specific shipment by AWB
 */
export async function getShipmentByAwb(awb: string, customerId: string): Promise<any | null> {
  const db = await getDatabase();

  try {
    const result = await db.getFirstAsync(
      `SELECT * FROM shipments WHERE awb = ? AND customerId = ?`,
      [awb, customerId]
    );
    return result || null;
  } catch (error) {
    console.error(`Failed to fetch shipment ${awb}:`, error);
    return null;
  }
}

/**
 * Get sync metadata for a resource
 */
export async function getSyncMetadata(resource: string): Promise<any | null> {
  const db = await getDatabase();

  try {
    const result = await db.getFirstAsync(
      `SELECT * FROM synced_metadata WHERE resource = ?`,
      [resource]
    );
    return result || null;
  } catch (error) {
    console.error(`Failed to fetch sync metadata for ${resource}:`, error);
    return null;
  }
}

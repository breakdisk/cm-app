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
        const result = await shipmentsService.createShipment({
          customer_name:  shipment.recipientName ?? 'Customer',
          customer_phone: shipment.recipientPhone ?? customerId,
          origin:         shipmentsService.parseAddress(shipment.origin),
          destination:    shipmentsService.parseAddress(shipment.destination),
          service_type:   'standard',
          weight_grams:   500,
          cod_amount_cents: shipment.codAmount ? Math.round(shipment.codAmount * 100) : undefined,
        });

        // Mark as synced with server-generated AWB
        await db.runAsync(
          `UPDATE shipments SET isPending = 0, syncedAt = ?, awb = ? WHERE id = ?`,
          [new Date().toISOString(), result.awb ?? result.tracking_number, shipment.id]
        );

        console.log(`Synced pending shipment: ${shipment.id}`);
      } catch (error) {
        console.error(`Failed to sync pending shipment ${shipment.awb}:`, error);
      }
    }

    // 2. Download latest shipments from API
    const response = await shipmentsService.listShipments({ limit: 100 });

    // Clear synced shipments and re-populate (keep pending ones)
    await db.runAsync(
      `DELETE FROM shipments WHERE customerId = ? AND isPending = 0`,
      [customerId]
    );

    for (const shipment of response.shipments) {
      const awb = shipment.awb ?? shipment.tracking_number ?? '';
      const originStr = typeof shipment.origin === 'object'
        ? `${shipment.origin.line1}, ${shipment.origin.city}`
        : String(shipment.origin ?? '');
      const destStr = typeof shipment.destination === 'object'
        ? `${shipment.destination.line1}, ${shipment.destination.city}`
        : String(shipment.destination ?? '');
      const fee = shipment.cod_amount_cents != null ? shipment.cod_amount_cents / 100 : 0;

      await db.runAsync(
        `INSERT OR REPLACE INTO shipments
         (id, awb, customerId, origin, destination, status, fee, currency, type, recipientName, recipientPhone, createdAt, syncedAt, isPending)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)`,
        [
          `${awb}-${Date.now()}`,
          awb,
          customerId,
          originStr,
          destStr,
          shipment.status ?? 'pending',
          fee,
          'PHP',
          'local',
          shipment.customer_name ?? '',
          shipment.customer_phone ?? '',
          shipment.created_at ?? new Date().toISOString(),
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

  const shipmentId = `pending-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
  const temporaryAwb = `PENDING-${Math.random().toString(36).substring(2, 8).toUpperCase()}`;

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

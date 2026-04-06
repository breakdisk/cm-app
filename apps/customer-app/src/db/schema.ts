/**
 * Database schema definitions for LogisticOS offline storage
 */

export const schema = [
  // Shipments table: stores local and synced shipments
  `
    CREATE TABLE IF NOT EXISTS shipments (
      id TEXT PRIMARY KEY,
      awb TEXT UNIQUE NOT NULL,
      customerId TEXT NOT NULL,
      origin TEXT NOT NULL,
      destination TEXT NOT NULL,
      status TEXT NOT NULL,
      fee REAL NOT NULL,
      currency TEXT NOT NULL,
      type TEXT NOT NULL,
      recipientName TEXT NOT NULL,
      recipientPhone TEXT NOT NULL,
      codAmount REAL,
      createdAt TEXT NOT NULL,
      syncedAt TEXT,
      isPending INTEGER DEFAULT 0,
      UNIQUE(awb)
    );
  `,

  // Tracking history: stores tracking updates and events for shipments
  `
    CREATE TABLE IF NOT EXISTS tracking_history (
      id TEXT PRIMARY KEY,
      awb TEXT NOT NULL,
      customerId TEXT NOT NULL,
      currentStatus TEXT NOT NULL,
      eta TEXT,
      currentLocation TEXT,
      events TEXT NOT NULL,
      lastUpdated TEXT NOT NULL,
      syncedAt TEXT,
      UNIQUE(awb, customerId)
    );
  `,

  // Saved addresses: stores customer's frequently used addresses
  `
    CREATE TABLE IF NOT EXISTS saved_addresses (
      id TEXT PRIMARY KEY,
      customerId TEXT NOT NULL,
      label TEXT NOT NULL,
      street TEXT NOT NULL,
      city TEXT NOT NULL,
      state TEXT NOT NULL,
      postalCode TEXT NOT NULL,
      country TEXT NOT NULL,
      isPrimary INTEGER DEFAULT 0,
      createdAt TEXT NOT NULL,
      UNIQUE(customerId, label)
    );
  `,

  // Synced metadata: tracks sync status and last sync time for each resource
  `
    CREATE TABLE IF NOT EXISTS synced_metadata (
      resource TEXT PRIMARY KEY,
      lastSyncedAt TEXT NOT NULL,
      syncStatus TEXT NOT NULL DEFAULT 'success'
    );
  `,
];

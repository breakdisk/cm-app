import * as SQLite from 'expo-sqlite';
import { schema } from './schema';

type DatabaseType = Awaited<ReturnType<typeof SQLite.openDatabaseAsync>>;

let db: DatabaseType | null = null;

/**
 * Initialize the SQLite database and create all required tables
 */
export async function initializeDatabase(): Promise<DatabaseType> {
  if (db) return db;

  try {
    db = await SQLite.openDatabaseAsync('logisticos_offline.db');

    // Create tables from schema
    for (const tableSql of schema) {
      await db.execAsync(tableSql);
    }

    return db;
  } catch (error) {
    console.error('Failed to initialize database:', error);
    throw error;
  }
}

/**
 * Get the database instance, initializing if necessary
 */
export async function getDatabase(): Promise<DatabaseType> {
  if (!db) {
    return initializeDatabase();
  }
  return db;
}

/**
 * Close the database connection
 */
export async function closeDatabase(): Promise<void> {
  if (db) {
    try {
      await db.closeAsync();
    } catch (error) {
      console.error('Error closing database:', error);
    } finally {
      db = null;
    }
  }
}

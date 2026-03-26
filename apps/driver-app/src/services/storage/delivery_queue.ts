/**
 * Offline-first delivery queue using Expo SQLite.
 * Driver actions (POD, status updates, location) are persisted locally
 * and synced to the API when connectivity is restored.
 */
import * as SQLite from "expo-sqlite";

export type QueuedActionType =
  | "delivery_completed"
  | "delivery_failed"
  | "pickup_completed"
  | "location_update"
  | "pod_captured"
  | "status_update";

export interface QueuedAction {
  id?: number;
  action_type: QueuedActionType;
  payload: string; // JSON-serialized action payload
  created_at: number; // Unix timestamp
  retry_count: number;
  last_error?: string;
}

class DeliveryQueueStorage {
  private db: SQLite.SQLiteDatabase | null = null;

  async open(): Promise<void> {
    this.db = await SQLite.openDatabaseAsync("logisticos_driver.db");
    await this.db.execAsync(`
      CREATE TABLE IF NOT EXISTS action_queue (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        action_type  TEXT NOT NULL,
        payload      TEXT NOT NULL,
        created_at   INTEGER NOT NULL,
        retry_count  INTEGER DEFAULT 0,
        last_error   TEXT
      );
      CREATE INDEX IF NOT EXISTS idx_queue_created ON action_queue(created_at);
    `);
  }

  async enqueue(action: Omit<QueuedAction, "id" | "retry_count">): Promise<void> {
    if (!this.db) throw new Error("Database not opened");
    await this.db.runAsync(
      `INSERT INTO action_queue (action_type, payload, created_at, retry_count)
       VALUES (?, ?, ?, 0)`,
      [action.action_type, action.payload, action.created_at]
    );
  }

  async dequeueAll(): Promise<QueuedAction[]> {
    if (!this.db) throw new Error("Database not opened");
    return await this.db.getAllAsync<QueuedAction>(
      "SELECT * FROM action_queue ORDER BY created_at ASC"
    );
  }

  async markSynced(id: number): Promise<void> {
    if (!this.db) throw new Error("Database not opened");
    await this.db.runAsync("DELETE FROM action_queue WHERE id = ?", [id]);
  }

  async markFailed(id: number, error: string): Promise<void> {
    if (!this.db) throw new Error("Database not opened");
    await this.db.runAsync(
      `UPDATE action_queue
       SET retry_count = retry_count + 1, last_error = ?
       WHERE id = ?`,
      [error, id]
    );
  }

  /** Remove actions that have failed more than 5 times (dead letter) */
  async pruneDeadLetters(): Promise<number> {
    if (!this.db) throw new Error("Database not opened");
    const result = await this.db.runAsync(
      "DELETE FROM action_queue WHERE retry_count > 5"
    );
    return result.changes;
  }
}

export const deliveryQueue = new DeliveryQueueStorage();

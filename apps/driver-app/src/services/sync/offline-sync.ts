/**
 * Offline Sync Service — Driver App
 *
 * When the driver is offline, actions (task completions, POD, location updates)
 * are persisted to SQLite via DeliveryQueueStorage.
 *
 * This service runs a sync loop that fires when the app regains connectivity,
 * draining the queue by replaying actions against the live API.
 *
 * Usage:
 *   offlineSync.start(getToken);  // call once in app root
 *   offlineSync.stop();           // call on app background/logout
 */
import NetInfo, { NetInfoState } from "@react-native-community/netinfo";
import { deliveryQueue, QueuedAction, QueuedActionType } from "../storage/delivery_queue";
import { tasksApi, CompleteTaskPayload, FailTaskPayload } from "../api/tasks";
import { podApi, SubmitPodPayload } from "../api/pod";
import { locationApi, LocationUpdate } from "../api/location";

const MAX_RETRIES = 5;
/** ms between sync attempts when online */
const SYNC_INTERVAL_MS = 15_000;

type TokenGetter = () => string | null;

// ── Action payload shapes (must match what enqueue() serialises) ───────────────

interface CompleteTaskAction {
  task_id: string;
  payload: CompleteTaskPayload;
}

interface FailTaskAction {
  task_id: string;
  payload: FailTaskPayload;
}

interface PodCapturedAction {
  session_id: string;
  payload: SubmitPodPayload;
}

interface LocationUpdateAction extends LocationUpdate {}

interface StatusUpdateAction {
  online: boolean;
}

// ── Dispatcher: route each action type to the correct API call ────────────────

async function dispatchAction(
  action: QueuedAction,
  token: string
): Promise<void> {
  const parsed = JSON.parse(action.payload);

  switch (action.action_type as QueuedActionType) {
    case "delivery_completed": {
      const { task_id, payload } = parsed as CompleteTaskAction;
      await tasksApi.complete(task_id, payload, token);
      break;
    }
    case "delivery_failed": {
      const { task_id, payload } = parsed as FailTaskAction;
      await tasksApi.fail(task_id, payload, token);
      break;
    }
    case "pickup_completed": {
      const { task_id, payload } = parsed as CompleteTaskAction;
      await tasksApi.complete(task_id, payload, token);
      break;
    }
    case "pod_captured": {
      const { session_id, payload } = parsed as PodCapturedAction;
      await podApi.submit(session_id, payload, token);
      break;
    }
    case "location_update": {
      const update = parsed as LocationUpdateAction;
      await locationApi.update(update, token);
      break;
    }
    case "status_update": {
      const { online } = parsed as StatusUpdateAction;
      if (online) {
        await locationApi.goOnline(token);
      } else {
        await locationApi.goOffline(token);
      }
      break;
    }
    default:
      throw new Error(`Unknown action type: ${action.action_type}`);
  }
}

// ── Sync loop ─────────────────────────────────────────────────────────────────

class OfflineSyncService {
  private getToken: TokenGetter = () => null;
  private interval: ReturnType<typeof setInterval> | null = null;
  private unsubscribeNetInfo: (() => void) | null = null;
  private syncing = false;

  /** Call once from the app root after the DB has been opened. */
  start(getToken: TokenGetter): void {
    this.getToken = getToken;

    // Subscribe to connectivity changes — sync immediately on reconnect
    this.unsubscribeNetInfo = NetInfo.addEventListener((state: NetInfoState) => {
      if (state.isConnected && state.isInternetReachable) {
        this.sync();
      }
    });

    // Also run on a polling interval (catches cases where connectivity
    // was already available when the listener registered)
    this.interval = setInterval(() => this.sync(), SYNC_INTERVAL_MS);

    // Initial sync in case we're already online
    this.sync();
  }

  stop(): void {
    if (this.interval) {
      clearInterval(this.interval);
      this.interval = null;
    }
    if (this.unsubscribeNetInfo) {
      this.unsubscribeNetInfo();
      this.unsubscribeNetInfo = null;
    }
  }

  /** Drain the queue — safe to call concurrently (guarded by syncing flag). */
  async sync(): Promise<void> {
    if (this.syncing) return;

    const token = this.getToken();
    if (!token) return;

    // Check connectivity before attempting
    const netState = await NetInfo.fetch();
    if (!netState.isConnected || !netState.isInternetReachable) return;

    this.syncing = true;
    try {
      // Prune dead letters first
      await deliveryQueue.pruneDeadLetters();

      const pending = await deliveryQueue.dequeueAll();
      if (pending.length === 0) return;

      for (const action of pending) {
        if ((action.retry_count ?? 0) >= MAX_RETRIES) continue; // will be pruned next cycle

        try {
          await dispatchAction(action, token);
          await deliveryQueue.markSynced(action.id!);
        } catch (err) {
          const message = err instanceof Error ? err.message : String(err);
          await deliveryQueue.markFailed(action.id!, message);
        }
      }
    } finally {
      this.syncing = false;
    }
  }

  /** Convenience: enqueue a delivery completion offline. */
  async enqueueCompleteTask(taskId: string, payload: CompleteTaskPayload): Promise<void> {
    await deliveryQueue.enqueue({
      action_type: "delivery_completed",
      payload: JSON.stringify({ task_id: taskId, payload }),
      created_at: Date.now(),
    });
  }

  async enqueueFailTask(taskId: string, payload: FailTaskPayload): Promise<void> {
    await deliveryQueue.enqueue({
      action_type: "delivery_failed",
      payload: JSON.stringify({ task_id: taskId, payload }),
      created_at: Date.now(),
    });
  }

  async enqueuePickupCompleted(taskId: string, payload: CompleteTaskPayload): Promise<void> {
    await deliveryQueue.enqueue({
      action_type: "pickup_completed",
      payload: JSON.stringify({ task_id: taskId, payload }),
      created_at: Date.now(),
    });
  }

  async enqueuePodCapture(sessionId: string, payload: SubmitPodPayload): Promise<void> {
    await deliveryQueue.enqueue({
      action_type: "pod_captured",
      payload: JSON.stringify({ session_id: sessionId, payload }),
      created_at: Date.now(),
    });
  }

  async enqueueLocationUpdate(update: LocationUpdate): Promise<void> {
    await deliveryQueue.enqueue({
      action_type: "location_update",
      payload: JSON.stringify(update),
      created_at: Date.now(),
    });
  }

  async enqueueStatusUpdate(online: boolean): Promise<void> {
    await deliveryQueue.enqueue({
      action_type: "status_update",
      payload: JSON.stringify({ online }),
      created_at: Date.now(),
    });
  }
}

export const offlineSync = new OfflineSyncService();

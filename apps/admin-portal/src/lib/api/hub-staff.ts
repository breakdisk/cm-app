import { createApiClient } from './client';

// ── Types ─────────────────────────────────────────────────────────────────────

export interface HubDriver {
  id:         string;
  user_id:    string;
  first_name: string;
  last_name:  string;
  phone:      string;
  status:     string;
  hub_id:     string | null;
}

// ── API factory ───────────────────────────────────────────────────────────────

export function createHubStaffApi() {
  const http = createApiClient();

  /** Drivers currently assigned to a specific hub (hub_id = hubId). */
  async function listHubScanners(hubId: string): Promise<HubDriver[]> {
    const res = await http.get<{ data: HubDriver[] }>(`/v1/drivers?hub_id=${encodeURIComponent(hubId)}`);
    return res.data.data;
  }

  /** Search all drivers in the tenant by name or phone fragment. */
  async function searchDrivers(query: string): Promise<HubDriver[]> {
    const res = await http.get<{ data: HubDriver[] }>(`/v1/drivers?search=${encodeURIComponent(query)}`);
    return res.data.data;
  }

  /**
   * Assign a driver as a hub scanner for the given hub.
   *
   * Identity-first dual-write with rollback:
   *   1. Grant hub_scanner role (identity) — if this fails, nothing is written.
   *   2. Set hub_id on driver-ops — if this fails, identity role is revoked and an
   *      error is thrown to surface the partial failure to the caller.
   */
  async function assignHubScanner(
    driverId: string,
    userId:   string,
    hubId:    string,
  ): Promise<void> {
    // Step 1 — identity (security gate first)
    await http.patch(`/v1/users/${userId}/roles`, {
      role:   'hub_scanner',
      action: 'assign',
    });
    // Step 2 — operational data
    try {
      await http.patch(`/v1/drivers/${driverId}`, { hub_id: hubId });
    } catch (err) {
      // Rollback: revoke the identity role so driver never has hub_id without role
      try {
        await http.patch(`/v1/users/${userId}/roles`, {
          role:   'hub_scanner',
          action: 'revoke',
        });
      } catch {
        console.error('[hub-staff] Role rollback failed after driver-ops write failure');
      }
      throw err;
    }
  }

  /**
   * Remove a hub scanner assignment.
   *
   * Identity-first: revoke the security role before clearing hub_id so the
   * driver loses scan access immediately. Rollback re-grants the role if
   * the second write fails.
   */
  async function removeHubScanner(
    driverId: string,
    userId:   string,
  ): Promise<void> {
    // Step 1 — revoke security role first
    await http.patch(`/v1/users/${userId}/roles`, {
      role:   'hub_scanner',
      action: 'revoke',
    });
    // Step 2 — clear hub_id
    try {
      await http.patch(`/v1/drivers/${driverId}`, { remove_hub_id: true });
    } catch (err) {
      // Rollback: re-grant the role so driver isn't left with hub_id but no role
      try {
        await http.patch(`/v1/users/${userId}/roles`, {
          role:   'hub_scanner',
          action: 'assign',
        });
      } catch {
        console.error('[hub-staff] Role rollback failed after driver-ops clear failure');
      }
      throw err;
    }
  }

  return { listHubScanners, searchDrivers, assignHubScanner, removeHubScanner };
}

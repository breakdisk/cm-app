"use client";
import { useState, useEffect, useCallback } from "react";

// Mirror of libs/auth/src/rbac.rs::default_permissions_for_role.
// Used as a fallback when the JWT doesn't include a `permissions` claim.
const ROLE_PERMISSIONS: Record<string, string[]> = {
  admin: [
    "shipments:create", "shipments:read", "shipments:update", "shipments:cancel", "shipments:bulk", "shipments:export",
    "dispatch:assign", "dispatch:reroute", "dispatch:view",
    "drivers:create", "drivers:read", "drivers:manage",
    "fleet:read", "fleet:manage",
    "payments:read", "payments:reconcile", "payments:export",
    "analytics:view", "analytics:export",
    "campaigns:create", "campaigns:send",
    "users:invite", "users:manage",
    "api_keys:manage",
    "carriers:manage", "carriers:read",
    "customers:read", "customers:manage",
    "compliance:review", "compliance:admin",
    "webhooks:read", "webhooks:manage",
  ],
  tenant_admin: [
    "shipments:create", "shipments:read", "shipments:update", "shipments:cancel", "shipments:bulk", "shipments:export",
    "dispatch:assign", "dispatch:reroute", "dispatch:view",
    "drivers:create", "drivers:read", "drivers:manage",
    "fleet:read", "fleet:manage",
    "payments:read", "payments:reconcile", "payments:export",
    "analytics:view", "analytics:export",
    "campaigns:create", "campaigns:send",
    "users:invite", "users:manage",
    "api_keys:manage",
    "carriers:manage", "carriers:read",
    "customers:read", "customers:manage",
    "compliance:review", "compliance:admin",
    "webhooks:read", "webhooks:manage",
  ],
  dispatcher: [
    "shipments:read", "shipments:update",
    "dispatch:assign", "dispatch:reroute", "dispatch:view",
    "drivers:read", "fleet:read",
  ],
  merchant: [
    "shipments:create", "shipments:read", "shipments:cancel", "shipments:bulk",
    "analytics:view", "customers:read",
  ],
  driver: [
    "shipments:read", "dispatch:view", "payments:cod-read-own",
  ],
  finance: [
    "payments:read", "payments:reconcile", "payments:export", "analytics:view",
  ],
  readonly: [
    "shipments:read", "dispatch:view", "analytics:view",
  ],
  customer: [
    "shipments:create", "shipments:read", "shipments:cancel",
  ],
  // Self-scoped: `carriers:manage-own` reaches only the carrier whose
  // contact_email matches this user. Not `carriers:manage`, which is
  // tenant-wide carrier authority.
  partner: [
    "carriers:read", "carriers:manage-own", "shipments:read", "payments:read", "analytics:view",
  ],
};

export interface UserIdentity {
  sub:        string | null;
  email:      string | null;
  first_name: string | null;
  last_name:  string | null;
  name:       string | null;
  roles:       string[];
  permissions: string[];
  tenant_id:  string | null;
}

export interface PermissionsHook {
  user:          UserIdentity | null;
  loading:       boolean;
  hasPermission: (perm: string) => boolean;
  hasRole:       (role: string) => boolean;
  displayName:   string;
  displayRole:   string;
}

const EMPTY_USER: UserIdentity = {
  sub: null, email: null, first_name: null, last_name: null,
  name: null, roles: [], permissions: [], tenant_id: null,
};

function expandPermissions(identity: UserIdentity): UserIdentity {
  if (identity.permissions.length > 0) return identity;
  const expanded = new Set<string>();
  for (const role of identity.roles) {
    for (const perm of (ROLE_PERMISSIONS[role] ?? [])) {
      expanded.add(perm);
    }
  }
  return { ...identity, permissions: Array.from(expanded) };
}

function basePath(): string {
  if (typeof window === "undefined") return "";
  const m = window.location.pathname.match(/^\/(merchant|admin|customer|partner)(?=\/|$)/);
  return m ? m[0] : "";
}

// Module-level cache so all hook instances share one fetch.
let cachedIdentity: UserIdentity | null = null;
let fetchInFlight: Promise<UserIdentity> | null = null;

function fetchIdentity(): Promise<UserIdentity> {
  if (!fetchInFlight) {
    fetchInFlight = fetch(`${basePath()}/api/auth/me`, {
      credentials: "same-origin",
      cache:       "no-store",
    })
      .then(async (res) => {
        if (!res.ok) return EMPTY_USER;
        const raw = (await res.json()) as UserIdentity;
        return expandPermissions(raw);
      })
      .catch(() => EMPTY_USER)
      .then((identity) => {
        cachedIdentity = identity;
        fetchInFlight  = null;
        return identity;
      });
  }
  return fetchInFlight;
}

export function usePermissions(): PermissionsHook {
  const [user, setUser]       = useState<UserIdentity | null>(cachedIdentity);
  const [loading, setLoading] = useState(!cachedIdentity);

  useEffect(() => {
    if (cachedIdentity) {
      setUser(cachedIdentity);
      setLoading(false);
      return;
    }
    fetchIdentity().then((identity) => {
      setUser(identity);
      setLoading(false);
    });
  }, []);

  const hasPermission = useCallback(
    (perm: string) => user?.permissions.includes(perm) ?? false,
    [user],
  );

  const hasRole = useCallback(
    (role: string) => user?.roles.includes(role) ?? false,
    [user],
  );

  const displayName = user
    ? (user.first_name && user.last_name
        ? `${user.first_name} ${user.last_name}`
        : user.name ?? user.email ?? "Ops User")
    : "Ops User";

  const displayRole = user?.roles.length
    ? user.roles[0].replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase())
    : "Admin";

  return { user, loading, hasPermission, hasRole, displayName, displayRole };
}

/** Call after sign-out to force a fresh fetch on next mount. */
export function clearPermissionsCache(): void {
  cachedIdentity = null;
  fetchInFlight  = null;
}

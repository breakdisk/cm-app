/**
 * Session refresh on 401.
 *
 * The access token lives 60 minutes. Before this existed, expiry was terminal:
 * every request came back `{"error":"Invalid or expired token"}`, the mesh run
 * reported "Lost the connection", and there was no route back — the app never
 * stored the refresh token the server had been returning all along, and had no
 * sign-out either.
 *
 * The single-flight property is the subtle one. A screen fires several requests
 * at once, and the server *rotates* the refresh token on use: if each 401
 * refreshed independently, the first would win and the rest would present a
 * token that had just been invalidated — signing the person out mid-session,
 * intermittently, in a way that looks like a server fault.
 */
import * as SecureStore from "expo-secure-store";

import { apiFetch } from "@/api/client";

jest.mock("expo-secure-store", () => {
  const store: Record<string, string> = {};
  return {
    getItemAsync: jest.fn(async (k: string) => store[k] ?? null),
    setItemAsync: jest.fn(async (k: string, v: string) => {
      store[k] = v;
    }),
    deleteItemAsync: jest.fn(async (k: string) => {
      delete store[k];
    }),
  };
});

const ok = (body: unknown) =>
  ({ ok: true, status: 200, json: async () => body, text: async () => "" }) as Response;
const unauthorised = () =>
  ({
    ok: false,
    status: 401,
    json: async () => ({}),
    text: async () => '{"error":"Invalid or expired token"}',
  }) as Response;

beforeEach(async () => {
  jest.clearAllMocks();
  await SecureStore.setItemAsync("auth_token", "expired-token");
  await SecureStore.setItemAsync("refresh_token", "refresh-1");
});

describe("apiFetch on an expired token", () => {
  it("refreshes and retries once, transparently to the caller", async () => {
    const calls: string[] = [];
    global.fetch = jest.fn(async (url: string) => {
      calls.push(String(url));
      if (String(url).endsWith("/v1/auth/refresh")) {
        return ok({ data: { access_token: "fresh", refresh_token: "refresh-2" } });
      }
      // First protected call 401s; the retry (after refresh) succeeds.
      return calls.filter((c) => c.includes("/v1/omnideliv/")).length === 1
        ? unauthorised()
        : ok({ orders: [] });
    }) as unknown as typeof fetch;

    await expect(apiFetch("/v1/omnideliv/orders")).resolves.toEqual({ orders: [] });
    expect(calls.filter((c) => c.endsWith("/v1/auth/refresh"))).toHaveLength(1);
    // Rotated: keeping refresh-1 would make the *next* refresh fail.
    await expect(SecureStore.getItemAsync("refresh_token")).resolves.toBe("refresh-2");
  });

  it("refreshes only once for concurrent 401s", async () => {
    let refreshes = 0;
    let firstRound = true;
    global.fetch = jest.fn(async (url: string) => {
      if (String(url).endsWith("/v1/auth/refresh")) {
        refreshes += 1;
        // Slow enough that all three callers are waiting on the same promise.
        await new Promise((r) => setTimeout(r, 10));
        return ok({ data: { access_token: "fresh", refresh_token: "refresh-2" } });
      }
      if (firstRound) return unauthorised();
      return ok({ done: true });
    }) as unknown as typeof fetch;

    const inflight = Promise.all([
      apiFetch("/v1/omnideliv/vendors").catch(() => null),
      apiFetch("/v1/omnideliv/catalog/search").catch(() => null),
      apiFetch("/v1/omnideliv/baskets").catch(() => null),
    ]);
    setTimeout(() => {
      firstRound = false;
    }, 5);
    await inflight;

    expect(refreshes).toBe(1);
  });

  it("gives up and clears the session when the refresh token is dead", async () => {
    global.fetch = jest.fn(async (url: string) =>
      String(url).endsWith("/v1/auth/refresh") ? unauthorised() : unauthorised(),
    ) as unknown as typeof fetch;

    await expect(apiFetch("/v1/omnideliv/orders")).rejects.toThrow();
    // Signed out rather than left in a loop that can never succeed.
    await expect(SecureStore.getItemAsync("refresh_token")).resolves.toBeNull();
  });
});

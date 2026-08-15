/**
 * The signed-in answer the root layout's gate reads.
 *
 * This exists because of a bug that made a *correct* OTP look rejected. The
 * gate in `app/_layout.tsx` read the token once on mount into React state.
 * Signing in writes the token and then routes to "/", at which point the gate
 * re-ran with its mount-time answer — still "signed out" — and replaced the
 * route back to "/sign-in". The customer typed the right code and landed back
 * on the code screen. It only came right after force-quitting the app, because
 * that ran the mount effect again, which is why "uninstall and reinstall"
 * appeared to fix it and why it kept coming back.
 *
 * So the property under test is not "the token is stored" — that always worked.
 * It is that **the answer the gate reads changes the moment sign-in succeeds**,
 * with no remount, no restart, and nothing to await.
 *
 * `expo-secure-store` is mocked: these assertions are about the cache the gate
 * consults, and a real keychain would make them a device test.
 */
import {
  isSignedIn,
  loadSession,
  signOut,
  verifyOtp,
  TOKEN_KEY,
} from "@/api/auth";

const mockStore = new Map<string, string>();

jest.mock("expo-secure-store", () => ({
  getItemAsync: jest.fn(async (k: string) => mockStore.get(k) ?? null),
  setItemAsync: jest.fn(async (k: string, v: string) => {
    mockStore.set(k, v);
  }),
  deleteItemAsync: jest.fn(async (k: string) => {
    mockStore.delete(k);
  }),
}));

/** A verify response shaped like identity's — body wrapped in `data`. */
function okVerify(token = "tok-abc") {
  return {
    ok: true,
    status: 200,
    json: async () => ({
      data: { access_token: token, refresh_token: "ref-abc", user: { id: "u1" } },
    }),
  } as unknown as Response;
}

beforeEach(async () => {
  mockStore.clear();
  await loadSession();
  jest.restoreAllMocks();
});

describe("the gate's signed-in answer", () => {
  it("is false before anyone signs in", () => {
    expect(isSignedIn()).toBe(false);
  });

  /**
   * The regression. Before the fix this assertion was the one that failed:
   * the token was in the store, and the gate still said no.
   */
  it("flips to true the instant verify succeeds, with no reload", async () => {
    jest.spyOn(global, "fetch").mockResolvedValue(okVerify());

    await verifyOtp("+639171234567", "123456");

    expect(isSignedIn()).toBe(true);
  });

  it("survives a restart — a primed cache reads what was stored", async () => {
    jest.spyOn(global, "fetch").mockResolvedValue(okVerify());
    await verifyOtp("+639171234567", "123456");

    // Simulate a cold start: the cache is gone, the keychain is not.
    await loadSession();

    expect(isSignedIn()).toBe(true);
    expect(mockStore.get(TOKEN_KEY)).toBe("tok-abc");
  });

  it("goes false on sign-out without waiting for a reload", async () => {
    jest.spyOn(global, "fetch").mockResolvedValue(okVerify());
    await verifyOtp("+639171234567", "123456");
    expect(isSignedIn()).toBe(true);

    await signOut();

    expect(isSignedIn()).toBe(false);
  });

  /**
   * A rejected code must not leave the gate believing otherwise — that would
   * let someone past the sign-in screen with no token, and every request behind
   * it would 401.
   */
  it("stays false when the code is refused", async () => {
    jest.spyOn(global, "fetch").mockResolvedValue({
      ok: false,
      status: 401,
      text: async () => "invalid code",
      json: async () => ({ error: "invalid code" }),
    } as unknown as Response);

    await expect(verifyOtp("+639171234567", "000000")).rejects.toThrow();
    expect(isSignedIn()).toBe(false);
  });

  /**
   * An unreadable keychain is the same as being signed out. Throwing here would
   * crash the layout before it could render the sign-in screen — the one screen
   * that could recover the situation.
   */
  it("treats an unreadable store as signed out rather than throwing", async () => {
    const secure = jest.requireMock("expo-secure-store") as {
      getItemAsync: jest.Mock;
    };
    secure.getItemAsync.mockRejectedValueOnce(new Error("keychain locked"));

    await expect(loadSession()).resolves.toBeUndefined();
    expect(isSignedIn()).toBe(false);
  });
});

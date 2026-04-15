/**
 * Browser-side authenticated fetch helper.
 *
 * Flow:
 *   1. Fetch `/api/token` (basePath-aware) to read the current access JWT
 *      from the httpOnly `__los_at` cookie. Cached in-memory until it 401s.
 *   2. Call the upstream request with `Authorization: Bearer <token>` and
 *      the `X-LogisticOS-Client: web` CSRF header.
 *   3. On 401, POST `/api/auth/refresh` to rotate the cookies, then retry.
 *   4. If refresh fails, redirect the tab to `/login?error=expired` — the
 *      landing app's login page knows how to handle that.
 *
 * The token cache is a single module-level promise: every concurrent caller
 * awaits the same inflight fetch, so we never spam `/api/token` or kick off
 * parallel refreshes.
 */

const CLIENT_HEADER_NAME  = "X-LogisticOS-Client";
const CLIENT_HEADER_VALUE = "web";

let cachedTokenPromise: Promise<string | null> | null = null;
let refreshInFlight: Promise<string | null> | null = null;

function basePathPrefix(): string {
  // basePath is set in next.config — prefer the runtime-exposed value so
  // helpers work across all four portals without per-app edits.
  if (typeof window === "undefined") return "";
  // Next exposes basePath on the `__NEXT_DATA__` global in SSR'd pages, but
  // for client-side routing the simplest source of truth is the path prefix
  // we're currently living under.
  const { pathname } = window.location;
  const match = pathname.match(/^\/(merchant|admin|customer|partner)(?=\/|$)/);
  return match ? match[0] : "";
}

async function fetchTokenFromCookie(): Promise<string | null> {
  const res = await fetch(`${basePathPrefix()}/api/token`, {
    method:      "GET",
    credentials: "same-origin",
    cache:       "no-store",
  });
  if (!res.ok) return null;
  const body = (await res.json()) as { access_token?: string };
  return body.access_token ?? null;
}

async function runRefresh(): Promise<string | null> {
  if (!refreshInFlight) {
    refreshInFlight = fetch(`${basePathPrefix()}/api/auth/refresh`, {
      method:      "POST",
      credentials: "same-origin",
      cache:       "no-store",
    })
      .then(async (res) => {
        if (!res.ok) return null;
        const body = (await res.json()) as { access_token?: string };
        return body.access_token ?? null;
      })
      .catch(() => null)
      .finally(() => {
        refreshInFlight = null;
      });
  }
  return refreshInFlight;
}

async function getAccessToken(forceRefresh = false): Promise<string | null> {
  if (!forceRefresh && cachedTokenPromise) {
    const t = await cachedTokenPromise;
    if (t) return t;
  }
  if (forceRefresh) {
    cachedTokenPromise = runRefresh();
  } else {
    cachedTokenPromise = fetchTokenFromCookie();
  }
  return cachedTokenPromise;
}

function redirectToLogin(): void {
  if (typeof window === "undefined") return;
  const returnTo = encodeURIComponent(window.location.pathname + window.location.search);
  window.location.href = `/login?error=expired&returnTo=${returnTo}`;
}

export interface AuthFetchOptions extends RequestInit {
  /** Skip the 401 → refresh retry dance. Used internally to avoid loops. */
  _noRetry?: boolean;
}

/**
 * Drop-in replacement for `fetch` that stamps the LogisticOS access JWT and
 * CSRF header, refreshing once on 401. Call this for every API request
 * against LogisticOS services.
 */
export async function authFetch(input: RequestInfo | URL, init: AuthFetchOptions = {}): Promise<Response> {
  const token = await getAccessToken();

  const headers = new Headers(init.headers);
  if (token) headers.set("Authorization", `Bearer ${token}`);
  headers.set(CLIENT_HEADER_NAME, CLIENT_HEADER_VALUE);
  if (!headers.has("Content-Type") && init.body && !(init.body instanceof FormData)) {
    headers.set("Content-Type", "application/json");
  }

  const res = await fetch(input, { ...init, headers, credentials: "same-origin" });

  if (res.status !== 401 || init._noRetry) return res;

  // Token expired mid-flight — refresh once and retry.
  const refreshed = await getAccessToken(true);
  if (!refreshed) {
    redirectToLogin();
    return res;
  }

  const retryHeaders = new Headers(init.headers);
  retryHeaders.set("Authorization", `Bearer ${refreshed}`);
  retryHeaders.set(CLIENT_HEADER_NAME, CLIENT_HEADER_VALUE);
  if (!retryHeaders.has("Content-Type") && init.body && !(init.body instanceof FormData)) {
    retryHeaders.set("Content-Type", "application/json");
  }
  return fetch(input, { ...init, headers: retryHeaders, credentials: "same-origin", _noRetry: true } as AuthFetchOptions);
}

/** Clear the in-memory token cache. Call after signout. */
export function clearAuthCache(): void {
  cachedTokenPromise = null;
  refreshInFlight    = null;
}

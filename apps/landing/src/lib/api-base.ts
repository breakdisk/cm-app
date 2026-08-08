/**
 * The backend origin, for code that runs in the browser.
 *
 * `NEXT_PUBLIC_*` is inlined at **build** time, so this is only a real value if
 * the Dockerfile passes it as a build arg — see `apps/landing/Dockerfile`. It
 * did not, which is why the shipped image had `http://localhost:8000` compiled
 * into every auth page: the fallback below, frozen into the bundle, asking each
 * visitor's own machine for an API that was never there.
 *
 * Server-side code does not belong here. It reads its own runtime variables
 * (see `identity-client.ts`), which are genuinely read at runtime and can point
 * at an internal address the browser could never reach.
 */
export const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

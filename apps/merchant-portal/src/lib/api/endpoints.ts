/**
 * The origins a browser is allowed to talk to.
 *
 * `NEXT_PUBLIC_*` is inlined into the client bundle at **build** time, not read
 * from the environment at runtime. A variable that is not passed as a Docker
 * build arg therefore compiles to its fallback, and no amount of container env
 * will ever change it.
 *
 * Three pages learned this the expensive way. Each read its own
 * `NEXT_PUBLIC_<SERVICE>_URL` that nothing set, so the production bundle
 * shipped with `http://localhost:8091` and `http://localhost:8007` literally
 * compiled in — every visitor's browser was asking *its own machine* for the
 * vendor catalog. The page rendered, the console looked fine, and every request
 * failed against a host that only existed on a developer's laptop.
 *
 * The rule that stops it recurring: the browser knows two origins. Everything
 * else is a path.
 */

/**
 * Every authenticated API call. The gateway resolves `/v1/*` to the right
 * service, so adding a backend service needs no new variable here — and
 * critically, this one *is* a build arg (see `apps/merchant-portal/Dockerfile`),
 * so it holds a real value in a built image.
 */
export const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

/**
 * An absolute link to the public tracking page, for handing to a customer.
 *
 * Same-origin by construction: the portal is served under `basePath: /merchant`
 * by the landing app, which owns `/track` on that same host. So the origin the
 * merchant is looking at is already the origin their customer needs, and
 * deriving it beats configuring it — a hardcoded domain here would be wrong for
 * every white-label tenant, and this link is the one artefact that leaves the
 * building.
 *
 * `NEXT_PUBLIC_PUBLIC_BASE_URL` overrides it for the case where the portal is
 * served on its own domain rather than behind the landing router.
 */
export function trackingPageUrl(trackingNumber: string): string {
  const configured = process.env.NEXT_PUBLIC_PUBLIC_BASE_URL;
  const origin =
    configured ?? (typeof window !== "undefined" ? window.location.origin : "");
  return `${origin}/track?awb=${encodeURIComponent(trackingNumber)}`;
}

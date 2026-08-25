import { authFetch } from "@/lib/auth/auth-fetch";

const BASE = process.env.NEXT_PUBLIC_API_BASE ?? "";

export interface ComplianceProfile {
  id:               string;
  entity_type:      string;
  entity_id:        string;
  overall_status:   string;
  jurisdiction:     string;
  last_reviewed_at: string | null;
  suspended_at:     string | null;
}

export interface DriverDocument {
  id:                    string;
  compliance_profile_id: string;
  document_type_id:      string;
  document_number:       string;
  expiry_date:           string | null;
  file_url:              string;
  status:                string;
  rejection_reason:      string | null;
  reviewed_by:           string | null;
  reviewed_at:           string | null;
  submitted_at:          string;
}

/**
 * A queue row. The document, plus who it belongs to.
 *
 * The server flattens the document's own fields into this object, so it stays a
 * superset of `DriverDocument` — the identity fields are the addition.
 *
 * `entity_id` is as far as compliance goes. It does not know anyone's name:
 * those live in `field_ops.couriers` and `driver_ops.drivers`, and a service
 * may not join across that boundary. Resolving it to a person is this portal's
 * job, because it is the one place that already holds every roster.
 */
export interface PendingReviewItem extends DriverDocument {
  entity_id:      string;
  /** `driver` for anyone who carries things — courier and driver alike. */
  entity_type:    string;
  jurisdiction:   string;
  overall_status: string;
}

/**
 * A kind of document a jurisdiction demands. Seeded by migration and identical
 * in every environment, so it is safe to fetch once and cache for the session.
 */
export interface DocumentType {
  id:                string;
  code:              string;
  jurisdiction:      string;
  name:              string;
  description:       string | null;
  is_required:       boolean;
  has_expiry:        boolean;
  warn_days_before:  number;
}

async function okJson(r: Response) {
  const j = await r.json().catch(() => ({}));
  // Backend wraps errors as { "error": { "code": "...", "message": "..." } }
  if (!r.ok) throw new Error(j?.error?.message ?? j?.message ?? `HTTP ${r.status}`);
  return j;
}

export async function fetchReviewQueue(): Promise<PendingReviewItem[]> {
  const j = await okJson(await authFetch(`${BASE}/api/v1/compliance/admin/queue?limit=50`));
  return j.data ?? [];
}

export async function fetchDocumentTypes(): Promise<DocumentType[]> {
  const j = await okJson(await authFetch(`${BASE}/api/v1/compliance/admin/document-types`));
  return j.data ?? [];
}

/**
 * The document itself, as an object URL the panel can render.
 *
 * Call this on a click, never on render: the server writes an audit row for each
 * call, because reading someone's identity document is a privacy-relevant act.
 * One row per deliberate open is evidence; one per render would be noise.
 *
 * **Why bytes and not a presigned link.** `file_url` holds an `s3://` URI a
 * browser cannot open, and presigning it does not help here — the signature is
 * against `STORAGE__ENDPOINT`, which is `http://minio:9000` on this deployment:
 * a compose-network hostname with no published port and no ingress. A reviewer's
 * browser cannot resolve it, so the "fixed" link named somewhere that does not
 * exist for them. Same wall the OmniDeliv catalog photos hit, resolved the same
 * way.
 *
 * And why a blob rather than pointing `<img src>` at the route: an `<img>` tag
 * sends no `Authorization` header, and unlike a product photo a KYC document
 * cannot be served unauthenticated.
 *
 * The caller owns the returned URL and must `URL.revokeObjectURL` it.
 */
export async function fetchDocumentBlobUrl(docId: string): Promise<string> {
  const r = await authFetch(`${BASE}/api/v1/compliance/admin/documents/${docId}/content`);
  if (!r.ok) {
    const j = await r.json().catch(() => ({}));
    throw new Error(j?.error?.message ?? j?.message ?? `HTTP ${r.status}`);
  }
  const blob = await r.blob();
  if (blob.size === 0) throw new Error("The server returned an empty document.");
  return URL.createObjectURL(blob);
}

/**
 * Is there a document to fetch a link for?
 *
 * Mirrors the server's `is_presignable`. Anything this service stored is
 * `s3://bucket/key`; a caller-hosted `http(s)://` URL is already openable and
 * the seeded mocks use `#`. Used to decide between a View button that presigns
 * and a plain link — or neither.
 */
export function isStoredObject(fileUrl: string | null | undefined): boolean {
  return typeof fileUrl === "string" && fileUrl.startsWith("s3://");
}

export async function fetchProfiles(): Promise<ComplianceProfile[]> {
  const j = await okJson(await authFetch(`${BASE}/api/v1/compliance/admin/profiles`));
  return j.data ?? [];
}

export async function fetchProfile(profileId: string) {
  const j = await okJson(await authFetch(`${BASE}/api/v1/compliance/admin/profiles/${profileId}`));
  return j.data;
}

export async function approveDocument(docId: string): Promise<void> {
  await okJson(await authFetch(`${BASE}/api/v1/compliance/admin/documents/${docId}/approve`, {
    method: "POST",
  }));
}

export async function rejectDocument(docId: string, reason: string): Promise<void> {
  await okJson(await authFetch(`${BASE}/api/v1/compliance/admin/documents/${docId}/reject`, {
    method: "POST",
    body: JSON.stringify({ reason }),
  }));
}

export async function suspendProfile(profileId: string, reason?: string): Promise<void> {
  await okJson(await authFetch(`${BASE}/api/v1/compliance/admin/profiles/${profileId}/suspend`, {
    method: "POST",
    body: JSON.stringify({ reason: reason ?? null }),
  }));
}

export async function reinstateProfile(profileId: string): Promise<void> {
  await okJson(await authFetch(`${BASE}/api/v1/compliance/admin/profiles/${profileId}/reinstate`, {
    method: "POST",
  }));
}

/**
 * Compliance API — KYC document submission.
 *
 * Backend: services/compliance
 *
 * Preferred flow (presigned R2 upload — direct client-to-R2):
 *   1. POST /api/v1/compliance/me/documents/upload-url  → { upload_url, s3_key, upload_headers }
 *   2. PUT  <upload_url>  with raw file bytes (no base64 overhead, uploads directly to R2)
 *   3. POST /api/v1/compliance/me/documents/confirm     → DriverDocument record created
 *
 * Legacy flow (kept for backwards compatibility):
 *   POST /api/v1/compliance/me/documents/upload  (base64 + metadata → server decodes → S3)
 *
 * Other endpoints:
 *   GET  /api/v1/compliance/me/documents/:id
 *   GET  /api/v1/compliance/me/profile
 */
import { createApiClient } from './client';

let cachedComplianceClient: ReturnType<typeof createApiClient> | null = null;

function getComplianceClient() {
  if (!cachedComplianceClient) {
    cachedComplianceClient = createApiClient(
      process.env.EXPO_PUBLIC_COMPLIANCE_URL ||
      process.env.EXPO_PUBLIC_API_URL ||
      'http://localhost:8017'
    );
  }
  return cachedComplianceClient;
}

export type DocumentStatus = 'pending' | 'approved' | 'rejected' | 'expired';
export type ContentType = 'image/jpeg' | 'image/png' | 'image/webp' | 'application/pdf';

export interface DriverDocument {
  id: string;
  compliance_profile_id: string;
  document_type_id: string;
  document_number: string;
  issue_date?: string | null;
  expiry_date?: string | null;
  file_url: string;            // s3:// URI — not directly browsable
  status: DocumentStatus;
  submitted_at: string;
  reviewed_at?: string | null;
}

export interface UploadDocumentPayload {
  /** Friendly code — "passport", "emirates_id", "drivers_license", etc. */
  document_type_code: string;
  document_number: string;
  file_base64: string;
  content_type: ContentType;
  issue_date?: string;         // YYYY-MM-DD
  expiry_date?: string;        // YYYY-MM-DD
}

/** Response from POST /me/documents/upload-url */
export interface UploadUrlResponse {
  upload_url:     string;
  s3_key:         string;
  /** Headers the client MUST include in the PUT request to R2. */
  upload_headers: Record<string, string>;
  expires_in:     number;
}

/** Payload for POST /me/documents/confirm (after direct R2 upload). */
export interface ConfirmDocumentPayload {
  document_type_code?: string;
  document_type_id?:   string;
  document_number:     string;
  /** Key returned by upload-url — pass back after successful PUT to R2. */
  s3_key:              string;
  content_type:        ContentType;
  issue_date?:         string;  // YYYY-MM-DD
  expiry_date?:        string;  // YYYY-MM-DD
}

export interface ComplianceProfile {
  id: string;
  tenant_id: string;
  entity_type: string;
  entity_id: string;
  jurisdiction: string;
  /** Rust serde field: overall_status (snake_case).
   *  Values: pending_submission | under_review | compliant |
   *          expiring_soon | expired | rejected | suspended */
  overall_status: string;
  created_at: string;
}

/** Map backend overall_status → app KycStatus. */
export function backendStatusToKyc(
  status: string,
): 'none' | 'pending' | 'verified' | 'rejected' {
  switch (status) {
    case 'compliant':
    case 'expiring_soon':
      return 'verified';
    case 'under_review':
      return 'pending';
    case 'rejected':
    case 'expired':
    case 'suspended':
      return 'rejected';
    default:
      // pending_submission or unknown → not yet verified
      return 'none';
  }
}

export const complianceApi = {
  /**
   * Step 1 of the presigned upload flow.
   * Returns a presigned R2 PUT URL + the s3_key to pass to confirmDocument.
   */
  async getUploadUrl(content_type: ContentType): Promise<UploadUrlResponse> {
    const { data } = await getComplianceClient().post<{ data: UploadUrlResponse }>(
      '/api/v1/compliance/me/documents/upload-url',
      { content_type },
    );
    return data.data;
  },

  /**
   * Step 3 of the presigned upload flow.
   * Call after a successful direct PUT to R2 to register the document record.
   */
  async confirmDocument(payload: ConfirmDocumentPayload): Promise<DriverDocument> {
    const { data } = await getComplianceClient().post<{ data: DriverDocument }>(
      '/api/v1/compliance/me/documents/confirm',
      payload,
    );
    return data.data;
  },

  /** Legacy base64-in-JSON upload (server-side). Kept for driver-app callers. */
  async uploadDocument(payload: UploadDocumentPayload): Promise<DriverDocument> {
    const { data } = await getComplianceClient().post<{ data: DriverDocument }>(
      '/api/v1/compliance/me/documents/upload',
      payload,
    );
    return data.data;
  },

  async getDocument(docId: string): Promise<DriverDocument> {
    const { data } = await getComplianceClient().get<{ data: DriverDocument }>(
      `/api/v1/compliance/me/documents/${docId}`,
    );
    return data.data;
  },

  async getProfile(): Promise<{ profile: ComplianceProfile; documents: DriverDocument[] }> {
    const { data } = await getComplianceClient().get<{
      data: { profile: ComplianceProfile; required_types: unknown[]; documents: DriverDocument[] };
    }>('/api/v1/compliance/me/profile');
    return { profile: data.data.profile, documents: data.data.documents };
  },
};

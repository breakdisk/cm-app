/**
 * Compliance API — KYC document submission.
 *
 * Backend: services/compliance
 *   POST /api/v1/compliance/me/documents/upload  (base64 + metadata → S3 + record)
 *   GET  /api/v1/compliance/me/documents/:id
 *   GET  /api/v1/compliance/me/profile
 *
 * Direct base64-in-JSON keeps the client simple (no multipart handling in
 * React Native) at the cost of ~33% bandwidth overhead vs raw bytes. Backend
 * caps uploads at 10MB so worst-case JSON payload is ~13.3MB.
 */
import { createApiClient } from './client';

let cachedComplianceClient: ReturnType<typeof createApiClient> | null = null;

function getComplianceClient() {
  if (!cachedComplianceClient) {
    cachedComplianceClient = createApiClient(
      process.env.EXPO_PUBLIC_COMPLIANCE_URL ||
      process.env.EXPO_PUBLIC_API_URL ||
      'http://localhost:8013'
    );
  }
  return cachedComplianceClient;
}

export type DocumentStatus = 'pending' | 'approved' | 'rejected' | 'expired';
export type ContentType = 'image/jpeg' | 'image/png' | 'application/pdf';

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

export interface ComplianceProfile {
  id: string;
  tenant_id: string;
  entity_type: string;
  entity_id: string;
  jurisdiction: string;
  status: string;
  created_at: string;
}

export const complianceApi = {
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

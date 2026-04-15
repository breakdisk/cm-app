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

export async function fetchReviewQueue(): Promise<DriverDocument[]> {
  const r = await authFetch(`${BASE}/api/v1/compliance/admin/queue?limit=50`);
  const j = await r.json();
  return j.data;
}

export async function fetchProfiles(): Promise<ComplianceProfile[]> {
  const r = await authFetch(`${BASE}/api/v1/compliance/admin/profiles`);
  const j = await r.json();
  return j.data;
}

export async function fetchProfile(profileId: string) {
  const r = await authFetch(`${BASE}/api/v1/compliance/admin/profiles/${profileId}`);
  return (await r.json()).data;
}

export async function approveDocument(docId: string): Promise<void> {
  await authFetch(`${BASE}/api/v1/compliance/admin/documents/${docId}/approve`, {
    method: "POST",
  });
}

export async function rejectDocument(docId: string, reason: string): Promise<void> {
  await authFetch(`${BASE}/api/v1/compliance/admin/documents/${docId}/reject`, {
    method: "POST",
    body: JSON.stringify({ reason }),
  });
}

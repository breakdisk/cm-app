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

export async function fetchReviewQueue(token: string): Promise<DriverDocument[]> {
  const r = await fetch(`${BASE}/api/v1/compliance/admin/queue?limit=50`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  const j = await r.json();
  return j.data;
}

export async function fetchProfiles(token: string): Promise<ComplianceProfile[]> {
  const r = await fetch(`${BASE}/api/v1/compliance/admin/profiles`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  const j = await r.json();
  return j.data;
}

export async function fetchProfile(token: string, profileId: string) {
  const r = await fetch(`${BASE}/api/v1/compliance/admin/profiles/${profileId}`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  return (await r.json()).data;
}

export async function approveDocument(token: string, docId: string): Promise<void> {
  await fetch(`${BASE}/api/v1/compliance/admin/documents/${docId}/approve`, {
    method: "POST",
    headers: { Authorization: `Bearer ${token}` },
  });
}

export async function rejectDocument(
  token: string,
  docId: string,
  reason: string,
): Promise<void> {
  await fetch(`${BASE}/api/v1/compliance/admin/documents/${docId}/reject`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ reason }),
  });
}

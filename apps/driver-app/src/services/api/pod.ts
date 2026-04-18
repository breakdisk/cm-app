import { apiRequest, POD_URL } from "./client";

export interface PodSession {
  id: string;
  shipment_id: string;
  status: "initiated" | "evidence_collected" | "submitted";
  has_signature: boolean;
  photo_count: number;
  otp_verified: boolean;
  submitted_at?: string;
}

export interface InitiatePodPayload {
  shipment_id: string;
  driver_lat: number;
  driver_lng: number;
}

export interface UploadUrlResponse {
  upload_url: string;
  photo_id: string;
}

export interface OtpResponse {
  otp_id: string;
}

export interface SubmitPodPayload {
  otp_code?: string;
  otp_id?: string;
  cod_collected_cents?: number;
  recipient_name?: string;
  notes?: string;
}

export const podApi = {
  initiate: (payload: InitiatePodPayload, token: string) =>
    apiRequest<{ data: PodSession }>("/v1/pod/initiate", {
      method: "POST",
      body: payload,
      token,
      baseUrl: POD_URL,
    }),

  getUploadUrl: (podId: string, contentType: string, token: string) =>
    apiRequest<{ data: UploadUrlResponse }>(`/v1/pod/${podId}/photo-upload-url`, {
      method: "POST",
      body: { content_type: contentType },
      token,
      baseUrl: POD_URL,
    }),

  attachPhoto: (podId: string, photoId: string, fileSizeBytes: number, token: string) =>
    apiRequest<void>(`/v1/pod/${podId}/photos/${photoId}`, {
      method: "POST",
      body: { file_size_bytes: fileSizeBytes },
      token,
      baseUrl: POD_URL,
    }),

  attachSignature: (podId: string, signatureBase64: string, token: string) =>
    apiRequest<void>(`/v1/pod/${podId}/signature`, {
      method: "POST",
      body: { signature_data: signatureBase64 },
      token,
      baseUrl: POD_URL,
    }),

  generateOtp: (podId: string, token: string) =>
    apiRequest<{ data: OtpResponse }>(`/v1/pod/${podId}/otp`, {
      method: "POST",
      token,
      baseUrl: POD_URL,
    }),

  submit: (podId: string, payload: SubmitPodPayload, token: string) =>
    apiRequest<{ data: PodSession }>(`/v1/pod/${podId}/submit`, {
      method: "POST",
      body: payload,
      token,
      baseUrl: POD_URL,
    }),

  get: (podId: string, token: string) =>
    apiRequest<{ data: PodSession }>(`/v1/pod/${podId}`, { token, baseUrl: POD_URL }),
};

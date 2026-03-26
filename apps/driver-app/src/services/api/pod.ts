import { apiRequest } from "./client";

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
  /** Initiate a POD session when driver arrives at delivery location */
  initiate: (payload: InitiatePodPayload, token: string) =>
    apiRequest<{ data: PodSession }>("/v1/pod/initiate", {
      method: "POST",
      body: payload,
      token,
    }),

  /** Get an upload URL for a delivery photo */
  getUploadUrl: (podId: string, contentType: string, token: string) =>
    apiRequest<{ data: UploadUrlResponse }>(
      `/v1/pod/${podId}/photo-upload-url`,
      {
        method: "POST",
        body: { content_type: contentType },
        token,
      }
    ),

  /** Notify the server that a photo has been uploaded (after S3 direct upload) */
  attachPhoto: (
    podId: string,
    photoId: string,
    fileSizeBytes: number,
    token: string
  ) =>
    apiRequest<void>(`/v1/pod/${podId}/photos/${photoId}`, {
      method: "POST",
      body: { file_size_bytes: fileSizeBytes },
      token,
    }),

  /** Upload a signature (base64-encoded PNG, max 500KB) */
  attachSignature: (podId: string, signatureBase64: string, token: string) =>
    apiRequest<void>(`/v1/pod/${podId}/signature`, {
      method: "POST",
      body: { signature_data: signatureBase64 },
      token,
    }),

  /** Request an OTP be sent to the customer's phone */
  generateOtp: (podId: string, token: string) =>
    apiRequest<{ data: OtpResponse }>(`/v1/pod/${podId}/otp`, {
      method: "POST",
      token,
    }),

  /** Submit the completed POD (finalizes delivery) */
  submit: (podId: string, payload: SubmitPodPayload, token: string) =>
    apiRequest<{ data: PodSession }>(`/v1/pod/${podId}/submit`, {
      method: "POST",
      body: payload,
      token,
    }),

  /** Get the current state of a POD session */
  get: (podId: string, token: string) =>
    apiRequest<{ data: PodSession }>(`/v1/pod/${podId}`, { token }),
};

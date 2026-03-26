import { apiRequest } from "./client";

export interface DriverTask {
  id: string;
  shipment_id: string;
  tracking_number: string;
  task_type: "pickup" | "delivery" | "return";
  status: "pending" | "in_progress" | "completed" | "failed";
  sequence: number;
  address: {
    line1: string;
    line2?: string;
    city: string;
    province: string;
    postal_code: string;
    lat: number;
    lng: number;
  };
  customer_name: string;
  customer_phone: string;
  special_instructions?: string;
  cod_amount?: number;
  eta?: string;
  completed_at?: string;
}

export interface CompleteTaskPayload {
  /** For POD: the pod_id from the POD service initiation */
  pod_id?: string;
  notes?: string;
}

export interface FailTaskPayload {
  reason: "no_answer" | "address_not_found" | "refused" | "access_denied" | "other";
  notes?: string;
}

export const tasksApi = {
  /** Get the driver's current task list (today's route) */
  list: (token: string) =>
    apiRequest<{ data: DriverTask[]; total: number }>("/v1/tasks", { token }),

  /** Get a single task */
  get: (taskId: string, token: string) =>
    apiRequest<{ data: DriverTask }>(`/v1/tasks/${taskId}`, { token }),

  /** Mark a task as in-progress (driver arrived at location) */
  start: (taskId: string, token: string) =>
    apiRequest<{ data: DriverTask }>(`/v1/tasks/${taskId}/start`, {
      method: "PUT",
      token,
    }),

  /** Complete a task (delivery or pickup done) */
  complete: (taskId: string, payload: CompleteTaskPayload, token: string) =>
    apiRequest<{ data: DriverTask }>(`/v1/tasks/${taskId}/complete`, {
      method: "PUT",
      body: payload,
      token,
    }),

  /** Fail a task (delivery attempted but unsuccessful) */
  fail: (taskId: string, payload: FailTaskPayload, token: string) =>
    apiRequest<{ data: DriverTask }>(`/v1/tasks/${taskId}/fail`, {
      method: "PUT",
      body: payload,
      token,
    }),
};

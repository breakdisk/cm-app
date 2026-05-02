import { createApiClient, ApiResponse, PaginatedApiResponse } from "./client";

export interface TenantUser {
  id: string;
  email: string;
  first_name: string;
  last_name: string;
  roles: string[];
  email_verified: boolean;
  is_active: boolean;
  created_at: string;
}

export interface InviteUserPayload {
  email: string;
  first_name: string;
  last_name: string;
  roles: string[];
  /** E.164 phone number — required for drivers so OTP login resolves to the
   *  pre-registered user rather than creating a duplicate ghost account. */
  phone_number?: string;
}

export interface InviteUserResult {
  user_id: string;
  email: string;
  temp_password: string;
}

export function createIdentityApi() {
  const client = createApiClient();

  return {
    inviteUser: (payload: InviteUserPayload) =>
      client
        .post<ApiResponse<InviteUserResult>>("/v1/users", payload)
        .then((r) => r.data),

    listUsers: (params?: { page?: number; per_page?: number }) =>
      client
        .get<PaginatedApiResponse<TenantUser>>("/v1/users", { params })
        .then((r) => r.data),

    getUser: (userId: string) =>
      client
        .get<ApiResponse<TenantUser>>(`/v1/users/${userId}`)
        .then((r) => r.data),
  };
}

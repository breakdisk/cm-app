import { describe, expect, jest, test } from "@jest/globals";

jest.mock("@/lib/api/client", () => ({ createApiClient: jest.fn() }));
jest.mock("sonner", () => ({ toast: { success: jest.fn(), error: jest.fn() } }));

import { profileProblem, type ProviderProfile } from "./ProviderProfileModal";

const lead = (over: Partial<ProviderProfile> = {}): ProviderProfile => ({
  service_lines: ["home_move"], coverage: ["local"], multi_truck_capable: false,
  fleet_trucks: 1, registered_helpers: 4, max_daily_jobs: 1, working_days: [0, 1, 2, 3, 4, 5], ...over,
});

describe("profileProblem mirrors the server's bounds", () => {
  test("a good home lead passes", () => {
    expect(profileProblem(lead())).toBeNull();
  });
  test("a job type and a coverage are required", () => {
    expect(profileProblem(lead({ service_lines: [] }))).toMatch(/job type/);
    expect(profileProblem(lead({ coverage: [] }))).toMatch(/local, international/);
  });
  test("a multi-truck lead has two trucks or more", () => {
    expect(profileProblem(lead({ multi_truck_capable: true }))).toMatch(/two trucks/);
    expect(profileProblem(lead({ multi_truck_capable: true, fleet_trucks: 2 }))).toBeNull();
  });
  test("counts are held to their ranges", () => {
    expect(profileProblem(lead({ max_daily_jobs: 5 }))).toMatch(/1 to 4/);
    expect(profileProblem(lead({ registered_helpers: 61 }))).toMatch(/0 to 60/);
  });
});

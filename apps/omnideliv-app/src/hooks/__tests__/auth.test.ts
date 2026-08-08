/**
 * Phone normalisation.
 *
 * Pure logic, tested here rather than through the screen because getting it
 * wrong is silent: the number is what identity keys the account on *and* what a
 * courier would dial. `09171234567` and `+639171234567` must be the same
 * person, or a returning customer gets a second empty account and the courier
 * gets a number that does not connect.
 *
 * Imported from `@/api/auth`, which pulls only `expo-secure-store` at module
 * load — safe under jest-expo, unlike `useMeshRun` (see sse.test.ts).
 */
import { normalisePhone } from "@/api/auth";

describe("normalisePhone", () => {
  /** The way people actually type it here. */
  it("turns a local 0-prefixed number into E.164", () => {
    expect(normalisePhone("09171234567")).toBe("+639171234567");
  });

  it("leaves an already-normalised number alone", () => {
    expect(normalisePhone("+639171234567")).toBe("+639171234567");
  });

  it("adds the missing plus to a country-coded number", () => {
    expect(normalisePhone("639171234567")).toBe("+639171234567");
  });

  /** Spaces, dashes and brackets are how phone numbers get written down. */
  it("ignores the punctuation people use", () => {
    expect(normalisePhone("0917 123 4567")).toBe("+639171234567");
    expect(normalisePhone("0917-123-4567")).toBe("+639171234567");
    expect(normalisePhone("(0917) 123 4567")).toBe("+639171234567");
  });

  /** The property that matters: every way of writing one number agrees. */
  it("maps every common spelling of one number to the same string", () => {
    const forms = [
      "09171234567",
      "0917 123 4567",
      "+63 917 123 4567",
      "639171234567",
    ];
    const normalised = new Set(forms.map(normalisePhone));
    expect([...normalised]).toEqual(["+639171234567"]);
  });

  /** A bare subscriber number is assumed local rather than rejected. */
  it("treats a bare number as local", () => {
    expect(normalisePhone("9171234567")).toBe("+639171234567");
  });
});

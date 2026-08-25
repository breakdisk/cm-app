import { describe, it, expect } from "@jest/globals";
import type { ComplianceProfile } from "@/lib/api/compliance";
import { profileRank, needsAttention, outstandingFirst, attentionCount } from "./profile-list";

function p(status: string, entityId = "e1"): ComplianceProfile {
  return {
    id:               `p-${status}-${entityId}`,
    entity_type:      "driver",
    entity_id:        entityId,
    overall_status:   status,
    jurisdiction:     "PH",
    last_reviewed_at: null,
    suspended_at:     null,
  };
}

describe("profileRank", () => {
  it("puts a stopped person above a merely idle one", () => {
    expect(profileRank("suspended")).toBeLessThan(profileRank("pending_submission"));
    expect(profileRank("rejected")).toBeLessThan(profileRank("pending_submission"));
  });

  it("puts pending_submission above under_review", () => {
    // under_review is already visible in the queue next door; listing it high
    // here would duplicate that tab rather than add anything.
    expect(profileRank("pending_submission")).toBeLessThan(profileRank("under_review"));
  });

  it("sorts compliant last", () => {
    for (const s of ["rejected", "suspended", "expired", "pending_submission", "under_review", "expiring_soon"]) {
      expect(profileRank(s)).toBeLessThan(profileRank("compliant"));
    }
  });

  it("does not let an unknown status outrank real work", () => {
    // A status a newer migration adds must not push a suspended courier down.
    expect(profileRank("some_future_status")).toBeGreaterThan(profileRank("pending_submission"));
    expect(profileRank("some_future_status")).toBeLessThan(profileRank("compliant"));
  });
});

describe("needsAttention", () => {
  it("counts the states a human still has to act on", () => {
    expect(needsAttention("pending_submission")).toBe(true);
    expect(needsAttention("under_review")).toBe(true);
    expect(needsAttention("suspended")).toBe(true);
    expect(needsAttention("expired")).toBe(true);
  });

  it("leaves alone the ones with nothing to do", () => {
    expect(needsAttention("compliant")).toBe(false);
    expect(needsAttention("expiring_soon")).toBe(false);
  });
});

describe("outstandingFirst", () => {
  const names = new Map([["a", "Ana Reyes"], ["b", "Ben Cruz"], ["c", "Cita Lim"]]);

  it("orders by what can be done about it", () => {
    const sorted = outstandingFirst(
      [p("compliant", "a"), p("pending_submission", "b"), p("suspended", "c")],
      names,
    );
    expect(sorted.map((x) => x.overall_status)).toEqual([
      "suspended", "pending_submission", "compliant",
    ]);
  });

  it("breaks ties by name so the list does not reshuffle on refresh", () => {
    const sorted = outstandingFirst(
      [p("pending_submission", "c"), p("pending_submission", "a"), p("pending_submission", "b")],
      names,
    );
    expect(sorted.map((x) => x.entity_id)).toEqual(["a", "b", "c"]);
  });

  it("falls back to the id when the roster could not name someone", () => {
    // The roster load is allowed to fail without breaking the console, so this
    // has to stay total rather than throwing on a missing name.
    const sorted = outstandingFirst([p("pending_submission", "zzz"), p("pending_submission", "a")], names);
    expect(sorted.map((x) => x.entity_id)).toEqual(["a", "zzz"]);
  });

  it("does not mutate the array the page holds", () => {
    const input = [p("compliant", "a"), p("suspended", "b")];
    const before = input.map((x) => x.id);
    outstandingFirst(input, names);
    expect(input.map((x) => x.id)).toEqual(before);
  });

  it("is empty-safe", () => {
    expect(outstandingFirst([], names)).toEqual([]);
  });
});

describe("attentionCount", () => {
  it("counts people waiting, not documents waiting", () => {
    // The whole point: a fleet that has submitted nothing shows a queue of zero
    // and reads as all clear. These three are the ones that were invisible.
    const profiles = [
      p("pending_submission", "a"),
      p("pending_submission", "b"),
      p("pending_submission", "c"),
      p("compliant", "d"),
    ];
    expect(attentionCount(profiles)).toBe(3);
  });

  it("is zero when everyone is settled", () => {
    expect(attentionCount([p("compliant", "a"), p("expiring_soon", "b")])).toBe(0);
  });
});

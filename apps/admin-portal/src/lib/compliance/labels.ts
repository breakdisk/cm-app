/**
 * Turning compliance's uuids into words.
 *
 * compliance deals in ids and nothing else, on purpose: it owns
 * `compliance_profiles` and `driver_documents`, and the two things a reviewer
 * actually needs to read — the courier's name and the document's name — live
 * elsewhere. The name is in `field_ops.couriers`, which a service may not join
 * to, and the document type is a catalogue behind its own endpoint.
 *
 * This portal is the one place that already holds every roster, so the join
 * happens here. Pure functions over data the callers fetched, so a failed
 * roster load degrades to ids rather than breaking the console.
 */
import type { DocumentType } from "@/lib/api/compliance";
import type { AdminCourier } from "@/lib/api/couriers";

/** Enough of a uuid to tell two rows apart, when there is nothing better. */
export function shortId(id: string | null | undefined): string {
  return (id ?? "").slice(0, 8) || "unknown";
}

/** `id → name`, e.g. `"Driver's Licence"`. */
export function buildTypeNames(types: DocumentType[]): Map<string, string> {
  return new Map(types.map((t) => [t.id, t.name]));
}

/**
 * What to call a document type.
 *
 * Falls back to the short id rather than to blank: a type this build has not
 * seen — a newer migration, a jurisdiction added since the catalogue was
 * cached — should still be distinguishable from the row above it.
 */
export function typeLabel(names: Map<string, string>, typeId: string): string {
  return names.get(typeId) ?? shortId(typeId);
}

/**
 * `entity_id → display name`, keyed both ways.
 *
 * `user_id` first, because that is what compliance stores: `entity_id` is the
 * identity user on both creation paths — `claims.user_id` on the lazy `/me`
 * path, and `driver_id` on the `driver.registered` event, which field-ops
 * publishes from `user_id`.
 *
 * `id` is indexed too as a second key. `register_courier` now forces
 * `courier.id = user_id` (the ADR-0015 collapse), so for anyone registered
 * since, the two are the same uuid and the second entry is a harmless
 * duplicate. Rows predating it are the reason to keep it.
 *
 * Couriers only. driver-ops drivers can also hold a compliance profile — both
 * roles map to entity type `driver` — but that tier's own `id`/`user_id`
 * split-brain is still open, so keying on it would be building on a bug. They
 * fall through to a short id.
 */
export function buildEntityNames(couriers: AdminCourier[]): Map<string, string> {
  const names = new Map<string, string>();
  for (const c of couriers) {
    const name = [c.first_name, c.last_name].filter(Boolean).join(" ").trim();
    if (!name) continue;
    if (c.user_id) names.set(c.user_id, name);
    if (c.id && !names.has(c.id)) names.set(c.id, name);
  }
  return names;
}

/** Who this profile belongs to, or the short id when nothing knows. */
export function entityLabel(names: Map<string, string>, entityId: string): string {
  return names.get(entityId) ?? shortId(entityId);
}

/**
 * Initials for the avatar.
 *
 * From the resolved name when there is one — "Juan dela Cruz" reads as "JD" —
 * and from the id otherwise, which is what the console did for every row.
 */
export function initialsFor(label: string): string {
  const words = label.trim().split(/\s+/).filter(Boolean);
  if (words.length >= 2) {
    return (words[0][0] + words[words.length - 1][0]).toUpperCase();
  }
  return label.slice(0, 2).toUpperCase();
}

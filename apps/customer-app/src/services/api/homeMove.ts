/**
 * Whole-home moving (`/v1/shipments/home/...`).
 *
 * The server owns every number: item volumes and weights (the catalogue),
 * the distance (it geocodes both addresses), the price, and the calendar
 * rule that a survey lands two days before the move. The helpers below are
 * for drawing — running totals while the customer lists rooms, a guard that
 * keeps the two date pickers from ever showing an illegal pair — and the
 * server re-checks all of it.
 */
import { getOrderClient } from './client';

export type PropertyType = 'apartment' | 'villa' | 'offices';
export type TruckPlan = 'trucks' | 'trips';

export interface HomeProperty {
  type: PropertyType;
  size: string;
  pickup_floor: number;
  pickup_has_lift: boolean;
  dropoff_floor: number;
  dropoff_has_lift: boolean;
  long_carry: boolean;
}

export interface CatalogueItem {
  key: string;
  group: string;
  name: string;
  volume_l: number;
  weight_kg: number;
  assembly: boolean;
  packing: boolean;
}

export interface CatalogueRoom {
  key: string;
  name: string;
  items: CatalogueItem[];
}

export interface HomeCatalogue {
  property_type: PropertyType;
  sizes: string[];
  rooms: CatalogueRoom[];
  truck_name: string;
}

export interface Slot {
  starts_at: string;
  ends_at: string;
  /** A team is free for it. Absent on an older server: treat as open. */
  open?: boolean;
  /** The window's price multiplier in bps; 10000 (or absent) is none. */
  surge_bps?: number;
}

export interface HomeSlots {
  survey: Slot[];
  move: Slot[];
  survey_lead_days: number;
  utc_offset_minutes: number;
  /** The first move start a team is free for. */
  next_open_move?: string | null;
  /** False when the server could not count teams and offered every window. */
  capacity_checked?: boolean;
}

export interface DeclaredItem {
  room: string;
  item_key: string;
  qty: number;
  dismantle: boolean;
  packing: boolean;
}

export interface HomeLine {
  key: string;
  label: string;
  note: string;
  amount_cents: number;
}

export interface HomeQuote {
  lines: HomeLine[];
  total_cents: number;
  volume_l: number;
  weight_kg: number;
  item_count: number;
  loads: number;
  trucks: number;
  trips_per_truck: number;
  drivers?: number;
  helpers: number;
  crew_total?: number;
  helper_hours: number;
  large_estate?: boolean;
  international?: boolean;
  survey_required: boolean;
  survey_cents?: number;
  truck_name: string;
  currency: string;
  distance_km: number;
  distance_basis?: import('./move').DistanceBasis;
  drive_minutes?: number | null;
  origin_text: string;
  destination_text: string;
  plan: TruckPlan;
  quote_token: string;
  expires_at: string;
}

export interface AddressLine {
  line1: string;
  city: string;
  country_code: string;
}

export const FLOORS = ['Ground', '1st', '2nd', '3rd', '4th', '5th +'];

/** The server's size lists, for reading a sentence before the catalogue is
 *  fetched. The catalogue response is the authority; these only pre-fill. */
export const SIZES_BY_TYPE: Record<PropertyType, string[]> = {
  apartment: ['Studio', '1 bedroom', '2 bedroom', '3 bedroom', '4 bedroom', '5 bedroom +'],
  villa: ['Studio', '1 bedroom', '2 bedroom', '3 bedroom', '4 bedroom', '5 bedroom', '6 bedroom +'],
  offices: ['Up to 10 desks', '10 to 30 desks', '30 to 75 desks', 'Whole floor'],
};

/** Null where home moves are not offered (503) — the entry points hide. */
export async function getCatalogue(type: PropertyType): Promise<HomeCatalogue | null> {
  try {
    const r = await getOrderClient().get<{ data: HomeCatalogue }>('/v1/shipments/home/catalogue', { params: { property_type: type } });
    return r.data.data;
  } catch (err: any) {
    if (err?.status === 503 || err?.status === 404) return null;
    throw err;
  }
}

/** Windows for this move: a Large Estate or a move abroad needs its own kind of team. */
export async function getSlots(opts: { large_estate?: boolean; international?: boolean; waitlist_id?: string } = {}): Promise<HomeSlots> {
  const params: Record<string, unknown> = { large_estate: !!opts.large_estate, international: !!opts.international };
  // Claiming a hold: the server leaves it out of the count, so it shows open.
  if (opts.waitlist_id) params.waitlist_id = opts.waitlist_id;
  const r = await getOrderClient().get<{ data: HomeSlots }>('/v1/shipments/home/slots', { params });
  return r.data.data;
}

function address(a: AddressLine) {
  return {
    line1: a.line1.trim(),
    city: a.city.trim(),
    province: a.city.trim(),
    postal_code: '0000',
    country_code: a.country_code.trim().toUpperCase(),
  };
}

export async function quoteHome(req: {
  origin: AddressLine;
  destination: AddressLine;
  property: HomeProperty;
  items: DeclaredItem[];
  plan: TruckPlan;
}): Promise<HomeQuote> {
  const r = await getOrderClient().post<HomeQuote>('/v1/shipments/home/quote', {
    ...req,
    origin: address(req.origin),
    destination: address(req.destination),
  });
  return r.data;
}

export interface HomeBooked {
  shipment: { id: string; awb?: string; tracking_number?: string };
  checkout_url?: string | null;
  home: { survey_at?: string | null; move_at: string; survey_required: boolean; total_cents: number; currency: string };
}

export async function bookHome(req: {
  quote_token: string;
  survey_at?: string | null;
  move_at: string;
  contact_name?: string;
  contact_phone?: string;
  notes?: string;
  intake?: unknown;
  idempotency_key: string;
  /** The surge shown for the window; the server refuses a higher one (SURGE_CHANGED). */
  accepted_surge_bps?: number;
  /** Booking a window held off the waitlist. */
  waitlist_id?: string;
}): Promise<HomeBooked> {
  const r = await getOrderClient().post<HomeBooked>('/v1/shipments/home', req);
  return r.data;
}

// ── A booked move ────────────────────────────────────────────────────────────

export interface BookedHomeMove {
  shipment_id: string;
  property: { type: PropertyType; size: string };
  items: { room: string; name: string; qty: number; volume_l: number }[];
  survey_required: boolean;
  survey_at?: string | null;
  move_at: string;
  total_cents: number;
  currency: string;
  trucks: number;
  helpers: number;
  crew_total: number;
  large_estate: boolean;
  survey_submitted_at?: string | null;
}

export interface SurveyExtra {
  kind: 'material' | 'resource';
  name: string;
  qty: number;
  unit_cents: number;
}

export interface Addendum {
  id: string;
  items: { room: string; name: string; qty: number; volume_l: number }[];
  extras: SurveyExtra[];
  note: string;
  items_cents: number;
  extras_cents: number;
  total_cents: number;
  currency: string;
  trucks: number;
  helpers: number;
  crew_total: number;
  status: 'pending' | 'approved' | 'declined' | 'paid';
  checkout_url?: string | null;
  created_at: string;
  /** The code to give the crew lead to approve on their phone. Sent to the
   *  customer only, while it is pending and not locked. */
  approval_code?: string | null;
}

/** Null when this shipment is not a whole-home move (or not yours). */
export async function getHomeMove(shipmentId: string): Promise<{ move: BookedHomeMove; lead: string | null } | null> {
  try {
    const r = await getOrderClient().get<{ data: BookedHomeMove; lead_name?: string | null }>(`/v1/shipments/${encodeURIComponent(shipmentId)}/home`);
    return { move: r.data.data, lead: r.data.lead_name ?? null };
  } catch (err: any) {
    if (err?.status === 404) return null;
    throw err;
  }
}

/** A survey photo: pre-existing condition evidence, or an access constraint. */
export interface SurveyPhoto {
  id: string;
  kind: 'condition' | 'access' | string;
  room?: string | null;
  caption: string;
  /** Viewable for an hour; absent when the media store can't be reached. */
  url?: string | null;
  created_at: string;
}

export async function getHomePhotos(shipmentId: string): Promise<SurveyPhoto[]> {
  try {
    const r = await getOrderClient().get<{ data: SurveyPhoto[] }>(`/v1/shipments/${shipmentId}/home/photos`);
    return r.data.data ?? [];
  } catch {
    return [];
  }
}

export async function getAddendum(shipmentId: string): Promise<Addendum | null> {
  const r = await getOrderClient().get<{ data: Addendum | null }>(`/v1/shipments/${encodeURIComponent(shipmentId)}/home/addendum`);
  return r.data.data;
}

/** Agree to the survey's additions: the difference is paid at the returned checkout. */
export async function approveAddendum(shipmentId: string, addendumId: string): Promise<string> {
  const r = await getOrderClient().post<{ data: { checkout_url: string } }>(
    `/v1/shipments/${encodeURIComponent(shipmentId)}/home/addendum/${encodeURIComponent(addendumId)}/approve`,
  );
  return r.data.data.checkout_url;
}

export async function declineAddendum(shipmentId: string, addendumId: string): Promise<void> {
  await getOrderClient().post(`/v1/shipments/${encodeURIComponent(shipmentId)}/home/addendum/${encodeURIComponent(addendumId)}/decline`);
}

/** "2 trucks & 7-person crew" — how the design names a team. */
export function crewLine(trucks: number, crew: number): string {
  return `${trucks} truck${trucks === 1 ? '' : 's'} & ${crew}-person crew`;
}

/** "Team Idris Kamal · 2 trucks & 7-person crew", or the crew alone before a lead takes it. */
export function teamLine(lead: string | null, trucks: number, crew: number): string {
  return lead ? `Team ${lead} · ${crewLine(trucks, crew)}` : crewLine(trucks, crew);
}

// ── Drawing helpers ──────────────────────────────────────────────────────────

/** Surveyed unless an apartment of studio or one bedroom (the server's rule). */
export function surveyRequired(p: Pick<HomeProperty, 'type' | 'size'>, sizes: string[]): boolean {
  return p.type !== 'apartment' || sizes.indexOf(p.size) >= 2;
}

/** A size from what a sentence stated, clamped to the top step of the list. */
export function sizeFromRead(type: PropertyType, sizes: string[], read: { bedrooms?: number | null; desks?: number | null; studio?: boolean }): string | null {
  if (type === 'offices') {
    const d = read.desks;
    if (d == null) return null;
    return d <= 10 ? sizes[0] : d <= 30 ? sizes[1] : d <= 75 ? sizes[2] : sizes[3];
  }
  if (read.studio) return sizes[0];
  const b = read.bedrooms;
  if (b == null) return null;
  if (b <= 0) return sizes[0];
  const exact = sizes.find((s) => s.startsWith(`${b} bedroom`));
  return exact ?? sizes[sizes.length - 1];
}

/** "Fri 18 Sep" in the calendar's own local time. */
export function slotDay(iso: string, offsetMin: number): string {
  const d = new Date(new Date(iso).getTime() + offsetMin * 60_000);
  const days = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
  const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  return `${days[d.getUTCDay()]} ${d.getUTCDate()} ${months[d.getUTCMonth()]}`;
}

export function slotHours(s: Slot, offsetMin: number): string {
  const hm = (iso: string) => {
    const d = new Date(new Date(iso).getTime() + offsetMin * 60_000);
    return `${String(d.getUTCHours()).padStart(2, '0')}:${String(d.getUTCMinutes()).padStart(2, '0')}`;
  };
  return `${hm(s.starts_at)} – ${hm(s.ends_at)}`;
}

function localDayNumber(iso: string, offsetMin: number): number {
  return Math.floor((new Date(iso).getTime() + offsetMin * 60_000) / 86_400_000);
}

/** Whether a move start may follow the chosen survey. */
export function moveOk(move: Slot, survey: Slot | null, required: boolean, slots: Pick<HomeSlots, 'survey_lead_days' | 'utc_offset_minutes'>): boolean {
  if (move.open === false) return false;
  if (!required) return true;
  if (!survey) return false;
  return localDayNumber(move.starts_at, slots.utc_offset_minutes) - localDayNumber(survey.starts_at, slots.utc_offset_minutes) >= slots.survey_lead_days;
}

/**
 * The move pick to show: the one chosen, while it is still legal after the
 * survey moved; otherwise the first legal one. Never trusts a stored index.
 */
/** The first survey window a lead is free for; 0 when none says otherwise. */
export function firstOpenSurvey(slots: HomeSlots): number {
  const i = slots.survey.findIndex((s) => s.open !== false);
  return i >= 0 ? i : 0;
}

export function validMovePick(slots: HomeSlots, surveyPick: number, movePick: number, required: boolean): number {
  const survey = required ? slots.survey[surveyPick] ?? null : null;
  const chosen = slots.move[movePick];
  if (chosen && moveOk(chosen, survey, required, slots)) return movePick;
  const first = slots.move.findIndex((m) => moveOk(m, survey, required, slots));
  return first >= 0 ? first : slots.move.length - 1;
}

export type Inventory = Record<string, Record<string, { qty: number; dismantle: boolean; packing: boolean }>>;

/** The draft inventory as the lines the quote takes. */
export function declared(inv: Inventory): DeclaredItem[] {
  return Object.entries(inv).flatMap(([room, items]) =>
    Object.entries(items)
      .filter(([, v]) => v.qty > 0)
      .map(([item_key, v]) => ({ room, item_key, qty: v.qty, dismantle: v.dismantle, packing: v.packing })),
  );
}

/** Running totals for drawing, from the served catalogue. The quote decides. */
export function totals(inv: Inventory, catalogue: HomeCatalogue | null): { volumeL: number; weightKg: number; count: number } {
  let volumeL = 0;
  let weightKg = 0;
  let count = 0;
  for (const room of catalogue?.rooms ?? []) {
    for (const [key, v] of Object.entries(inv[room.key] ?? {})) {
      const item = room.items.find((i) => i.key === key);
      if (!item || v.qty <= 0) continue;
      volumeL += item.volume_l * v.qty;
      weightKg += item.weight_kg * v.qty;
      count += v.qty;
    }
  }
  return { volumeL, weightKg, count };
}

export function roomTotals(inv: Inventory, room: CatalogueRoom): { volumeL: number; count: number } {
  let volumeL = 0;
  let count = 0;
  for (const [key, v] of Object.entries(inv[room.key] ?? {})) {
    const item = room.items.find((i) => i.key === key);
    if (!item || v.qty <= 0) continue;
    volumeL += item.volume_l * v.qty;
    count += v.qty;
  }
  return { volumeL, count };
}

export function m3(litres: number): string {
  return `${(litres / 1000).toFixed(1)} m³`;
}

// ── Reading a sentence offline (the handoff's Part B rules) ──────────────────

const DWELLING = '(?:house|home|flat|apartment|villa|townhouse|duplex|bungalow|penthouse|studio|condo|office|offices)';
const NUMBER_WORDS: Record<string, number> = { one: 1, two: 2, three: 3, four: 4, five: 5, six: 6, seven: 7 };

export interface HomeRead {
  property_type?: PropertyType;
  bedrooms?: number;
  desks?: number;
  studio?: boolean;
  whole_floor?: boolean;
  pickup_floor?: number;
  pickup_has_lift?: boolean;
}

/** Whether a sentence is about moving a whole home or office. */
export function isHomeMove(text: string): boolean {
  const t = text.toLowerCase();
  if (/\b(?:whole|entire|full)\s+(?:house|home|flat|apartment|villa|office)\b/.test(t)) return true;
  if (/\b\d+\s*-?\s*(?:bed|bedroom|bhk|br)s?\b/.test(t)) return true;
  if (/\bmoving\s+(?:out|house|home)\b/.test(t)) return true;
  const verb = '(?:mov\\w*|relocat\\w*|shift\\w*)';
  const near = new RegExp(`\\b${verb}\\b.{0,44}\\b${DWELLING}\\b|\\b${DWELLING}\\b.{0,44}\\b${verb}\\b`);
  return near.test(t);
}

/** What a home sentence states about the property. Only what it states. */
export function readHome(text: string): HomeRead {
  const t = text.toLowerCase();
  const read: HomeRead = {};
  if (/\b(?:office|offices|desk|desks|workspace|headquarters?)\b/.test(t)) read.property_type = 'offices';
  else if (/\b(?:villa|house|townhouse|duplex|bungalow)\b/.test(t)) read.property_type = 'villa';
  else if (/\b(?:apartment|flat|studio|condo|penthouse)\b/.test(t)) read.property_type = 'apartment';

  const bed = t.match(/\b(\d+|one|two|three|four|five|six|seven)\s*-?\s*(?:bed|bedroom|bhk|br)s?\b/);
  if (bed) read.bedrooms = /^\d+$/.test(bed[1]) ? Number(bed[1]) : NUMBER_WORDS[bed[1]];
  if (/\bstudio\b/.test(t)) read.studio = true;
  const desks = t.match(/\b(\d+)\s*desks?\b/);
  if (desks) read.desks = Number(desks[1]);
  if (/\bwhole\s+floor\b/.test(t)) {
    read.whole_floor = true;
    read.desks = read.desks ?? 999;
  }
  if (!read.property_type && read.desks != null) read.property_type = 'offices';
  if (!read.property_type && read.bedrooms != null) read.property_type = 'apartment';

  const floor = t.match(/\b(\d+)(?:st|nd|rd|th)\s+floor\b/);
  if (floor) read.pickup_floor = Math.min(5, Number(floor[1]));
  else if (/\bground\s+floor\b/.test(t)) read.pickup_floor = 0;
  if (/\b(?:no lift|without a lift|no elevator|walk-?up)\b/.test(t)) read.pickup_has_lift = false;
  else if (/\b(?:lift|elevator)\b/.test(t)) read.pickup_has_lift = true;
  return read;
}

/** The read-back chips: one per stated field, never an invented one. */
export function readChips(r: HomeRead, size: string | null): string[] {
  const chips: string[] = [];
  if (r.property_type) chips.push(r.property_type === 'offices' ? 'Offices' : r.property_type === 'villa' ? 'Villa' : 'Apartment');
  if (size) chips.push(size);
  if (r.pickup_floor != null) chips.push(`${FLOORS[r.pickup_floor]} floor`);
  if (r.pickup_has_lift != null) chips.push(r.pickup_has_lift ? 'Lift' : 'No lift');
  return chips;
}

// ── Surge ────────────────────────────────────────────────────────────────────

export const NO_SURGE_BPS = 10_000;

/** "High demand ×1.3" for a surging window; null otherwise. */
export function surgeLabel(bps?: number | null): string | null {
  if (!bps || bps <= NO_SURGE_BPS) return null;
  return `High demand ×${(bps / NO_SURGE_BPS).toFixed(1).replace(/\.0$/, '')}`;
}

/**
 * The total for a window: the fare surges, the survey fee never does. The
 * server's arithmetic, for showing the price before booking — the server
 * charges its own figure and refuses a surge above what was shown.
 */
export function surgedTotal(q: Pick<HomeQuote, 'total_cents' | 'survey_cents'>, bps?: number | null): number {
  const surge = Math.max(0, (bps ?? NO_SURGE_BPS) - NO_SURGE_BPS);
  const fare = Math.max(0, q.total_cents - (q.survey_cents ?? 0));
  return q.total_cents + Math.floor((fare * surge + 5_000) / 10_000);
}

/** The surge a SURGE_CHANGED refusal names, if any. */
export function surgeFromError(message: string): number | null {
  const m = message.match(/SURGE_CHANGED:(\d+)/);
  return m ? Number(m[1]) : null;
}

// ── The priority waitlist ────────────────────────────────────────────────────

export interface WaitlistPlace {
  id: string;
  wanted_date: string;
  status: 'waiting' | 'offered' | 'booked' | 'lapsed' | 'withdrawn';
  offered_move_at?: string | null;
  hold_expires_at?: string | null;
  /** Places ahead in the queue, while waiting. */
  ahead?: number | null;
}

export interface WaitlistClaim {
  waitlist_id: string;
  move_at: string;
  hold_expires_at: string;
  quote: HomeQuote;
}

/** Queue for a fully booked date (YYYY-MM-DD, local). */
export async function joinWaitlist(quoteToken: string, date: string): Promise<WaitlistPlace> {
  const r = await getOrderClient().post<{ data: WaitlistPlace }>('/v1/shipments/home/waitlist', { quote_token: quoteToken, date });
  return r.data.data;
}

export async function getWaitlist(): Promise<WaitlistPlace[]> {
  try {
    const r = await getOrderClient().get<{ data: WaitlistPlace[] }>('/v1/shipments/home/waitlist');
    return r.data.data ?? [];
  } catch (err: any) {
    if (err?.status === 404 || err?.status === 503) return [];
    throw err;
  }
}

export async function withdrawWaitlist(id: string): Promise<void> {
  await getOrderClient().delete(`/v1/shipments/home/waitlist/${id}`);
}

/** A window held for you: the move priced for it, and how long it is held. */
export async function claimWaitlist(id: string): Promise<WaitlistClaim> {
  const r = await getOrderClient().post<{ data: WaitlistClaim }>(`/v1/shipments/home/waitlist/${id}/claim`);
  return r.data.data;
}

/**
 * Local dates (YYYY-MM-DD) shown in the calendar where every move window is
 * full — the ones a customer can queue for.
 */
export function fullDates(slots: Pick<HomeSlots, 'move' | 'utc_offset_minutes'>): string[] {
  const byDay = new Map<string, boolean>();
  for (const m of slots.move) {
    const day = new Date(new Date(m.starts_at).getTime() + slots.utc_offset_minutes * 60_000).toISOString().slice(0, 10);
    byDay.set(day, (byDay.get(day) ?? false) || m.open !== false);
  }
  return [...byDay.entries()].filter(([, anyOpen]) => !anyOpen).map(([day]) => day);
}

/** Whole minutes and seconds left on a hold: "4:12", or null once it ends. */
export function holdLeft(expiresIso: string, now: number = Date.now()): string | null {
  const ms = new Date(expiresIso).getTime() - now;
  if (!(ms > 0)) return null;
  const s = Math.ceil(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

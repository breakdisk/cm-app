//! Whole-home moving: the property, the rooms, the arithmetic, the calendar.
//!
//! A third pricing path beside the parcel tariff and the rate card. One
//! vehicle class for every home move — a box truck — so sizing is a count of
//! loads and trucks, never a choice of vehicle. Every figure is priced here
//! from the server's own catalogue and the server's own distance: the client
//! says which items and how many, never how big or how far.
//!
//! Pure: nothing here reads a clock, a database or a network.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};
use serde::{Deserialize, Serialize};

// ── The property ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyType {
    Apartment,
    Villa,
    Offices,
}

impl PropertyType {
    pub fn sizes(self) -> &'static [&'static str] {
        match self {
            Self::Apartment => &["Studio", "1 bedroom", "2 bedroom", "3 bedroom", "4 bedroom", "5 bedroom +"],
            Self::Villa => &["Studio", "1 bedroom", "2 bedroom", "3 bedroom", "4 bedroom", "5 bedroom", "6 bedroom +"],
            Self::Offices => &["Up to 10 desks", "10 to 30 desks", "30 to 75 desks", "Whole floor"],
        }
    }

    /// `(key, name, catalogue group)`.
    pub fn rooms(self) -> &'static [(&'static str, &'static str, &'static str)] {
        match self {
            Self::Offices => &[
                ("reception", "Reception", "reception"),
                ("openplan", "Open plan floor", "desks"),
                ("meeting", "Meeting rooms", "meeting"),
                ("exec", "Executive offices", "desks"),
                ("server", "Server room", "server"),
                ("pantry", "Pantry", "office_util"),
                ("filing", "Filing and archive", "office_util"),
                ("storage", "Storage", "office_util"),
            ],
            Self::Apartment | Self::Villa => &[
                ("living", "Living room", "living"),
                ("kitchen", "Kitchen & dining", "kitchen"),
                ("master", "Master bedroom", "bed"),
                ("room1", "Bedroom 1", "bed"),
                ("room2", "Bedroom 2", "bed"),
                ("maids", "Maid's room", "bed"),
                ("garage", "Garage", "util"),
                ("basement", "Basement", "util"),
                ("storage", "Storage", "util"),
            ],
        }
    }

    pub fn group_for(self, room: &str) -> Option<&'static str> {
        self.rooms().iter().find(|r| r.0 == room).map(|r| r.2)
    }
}

/// Floors run Ground (0) to "5th +" (5).
pub const TOP_FLOOR: u8 = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Property {
    #[serde(rename = "type")]
    pub kind: PropertyType,
    /// One of `kind.sizes()`.
    pub size: String,
    pub pickup_floor: u8,
    pub pickup_has_lift: bool,
    pub dropoff_floor: u8,
    pub dropoff_has_lift: bool,
    /// Over 40 m from the door to the loading point.
    #[serde(default)]
    pub long_carry: bool,
}

impl Property {
    pub fn validate(&self) -> Result<(), String> {
        if !self.kind.sizes().contains(&self.size.as_str()) {
            return Err(format!("\"{}\" is not a size for this property type", self.size));
        }
        if self.pickup_floor > TOP_FLOOR || self.dropoff_floor > TOP_FLOOR {
            return Err("Floors run from 0 (ground) to 5 (fifth and above)".into());
        }
        Ok(())
    }

    /// Surveyed unless it is an apartment of studio or one bedroom — below
    /// that the customer's own inventory is taken as declared.
    pub fn survey_required(&self) -> bool {
        let index = self.kind.sizes().iter().position(|s| *s == self.size).unwrap_or(usize::MAX);
        self.kind != PropertyType::Apartment || index >= 2
    }

    /// Helpers the lead brings for this size, before access and truck rules.
    /// Studio to four bedrooms are the architect's table (1 to 5); the larger
    /// homes and the offices extend it and are defaults.
    pub fn size_helpers(&self) -> i64 {
        let index = self.kind.sizes().iter().position(|s| *s == self.size).unwrap_or(0);
        let table: &[i64] = match self.kind {
            PropertyType::Apartment => &[1, 2, 3, 4, 5, 6],
            PropertyType::Villa => &[1, 2, 3, 4, 5, 6, 7],
            PropertyType::Offices => &[3, 5, 7, 9],
        };
        table.get(index).copied().unwrap_or(1)
    }

    /// No lift above the second floor, at either end.
    pub fn walk_up(&self) -> bool {
        (!self.pickup_has_lift && self.pickup_floor > 2) || (!self.dropoff_has_lift && self.dropoff_floor > 2)
    }
}

// ── The catalogue ────────────────────────────────────────────────────────────

/// One thing a room may hold. Volume is packed, in litres; weight is per unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogueItem {
    pub key: String,
    pub group: String,
    pub name: String,
    pub volume_l: i32,
    pub weight_kg: i32,
    /// Offered for dismantle and rebuild, and ticked by default.
    pub assembly: bool,
    /// Needs special packing, ticked by default.
    pub packing: bool,
}

// ── The rates ────────────────────────────────────────────────────────────────

/// Deployment config, in the tenant's currency's minor units. All zero by
/// default, and home moves are refused until `trip_cents` is set — a home
/// move priced at nothing is worse than none.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HomeRates {
    #[serde(default)]
    pub trip_cents: i64,
    #[serde(default)]
    pub per_km_cents: i64,
    #[serde(default)]
    pub helper_hour_cents: i64,
    #[serde(default = "default_min_hours")]
    pub min_helper_hours: i64,
    #[serde(default = "default_m3_per_hour")]
    pub m3_per_helper_hour: i64,
    #[serde(default)]
    pub assembly_cents: i64,
    #[serde(default)]
    pub packing_cents: i64,
    #[serde(default)]
    pub survey_cents: i64,
    #[serde(default = "default_truck_l")]
    pub truck_volume_l: i64,
    #[serde(default = "default_usable_pct")]
    pub truck_usable_pct: i64,
    #[serde(default = "default_payload_kg")]
    pub truck_payload_kg: i64,
    #[serde(default = "default_truck_name")]
    pub truck_name: String,
    /// Local time for the slot calendar. No tenant time zone exists anywhere.
    #[serde(default = "default_utc_offset")]
    pub utc_offset_minutes: i32,
    /// Over this packed volume the move is a Large Estate: never one truck.
    #[serde(default = "default_large_estate_l")]
    pub large_estate_l: i64,
    /// Hours before the survey inside which cancelling keeps the survey fee.
    #[serde(default = "default_survey_refund_hours")]
    pub survey_refund_until_hours: i64,
}

fn default_large_estate_l() -> i64 { 75_000 }
fn default_survey_refund_hours() -> i64 { 12 }

fn default_min_hours() -> i64 { 4 }
fn default_m3_per_hour() -> i64 { 6 }
fn default_truck_l() -> i64 { 18_000 }
fn default_usable_pct() -> i64 { 85 }
fn default_payload_kg() -> i64 { 3_000 }
fn default_truck_name() -> String { "3-ton box truck".into() }
fn default_utc_offset() -> i32 { 480 }

impl Default for HomeRates {
    fn default() -> Self {
        Self {
            trip_cents: 0,
            per_km_cents: 0,
            helper_hour_cents: 0,
            min_helper_hours: default_min_hours(),
            m3_per_helper_hour: default_m3_per_hour(),
            assembly_cents: 0,
            packing_cents: 0,
            survey_cents: 0,
            truck_volume_l: default_truck_l(),
            truck_usable_pct: default_usable_pct(),
            truck_payload_kg: default_payload_kg(),
            truck_name: default_truck_name(),
            utc_offset_minutes: default_utc_offset(),
            large_estate_l: default_large_estate_l(),
            survey_refund_until_hours: default_survey_refund_hours(),
        }
    }
}

impl HomeRates {
    pub fn offered(&self) -> bool {
        self.trip_cents > 0
    }
}

// ── The inventory ────────────────────────────────────────────────────────────

/// What the customer declared, one line per item in a room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredItem {
    pub room: String,
    pub item_key: String,
    pub qty: u32,
    #[serde(default)]
    pub dismantle: bool,
    #[serde(default)]
    pub packing: bool,
}

pub const MAX_LINES: usize = 400;
pub const MAX_QTY: u32 = 200;

/// A declared line with the catalogue's size and weight on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PricedItem {
    pub room: String,
    pub item_key: String,
    pub name: String,
    pub qty: u32,
    pub volume_l: i32,
    pub weight_kg: i32,
    pub dismantle: bool,
    pub packing: bool,
}

/// Every declared line resolved against the catalogue, or the first reason
/// it cannot be.
pub fn resolve(property: &Property, declared: &[DeclaredItem], catalogue: &[CatalogueItem]) -> Result<Vec<PricedItem>, String> {
    if declared.is_empty() {
        return Err("Declare at least one item".into());
    }
    if declared.len() > MAX_LINES {
        return Err(format!("At most {MAX_LINES} lines"));
    }
    declared
        .iter()
        .map(|d| {
            let group = property
                .kind
                .group_for(&d.room)
                .ok_or_else(|| format!("\"{}\" is not a room in this property", d.room))?;
            if !(1..=MAX_QTY).contains(&d.qty) {
                return Err(format!("Quantity is 1–{MAX_QTY}"));
            }
            let item = catalogue
                .iter()
                .find(|c| c.key == d.item_key && c.group == group)
                .ok_or_else(|| format!("\"{}\" is not in the {} catalogue", d.item_key, d.room))?;
            Ok(PricedItem {
                room: d.room.clone(),
                item_key: item.key.clone(),
                name: item.name.clone(),
                qty: d.qty,
                volume_l: item.volume_l.max(0),
                weight_kg: item.weight_kg.max(0),
                dismantle: d.dismantle,
                packing: d.packing,
            })
        })
        .collect()
}

// ── The arithmetic ───────────────────────────────────────────────────────────

/// One truck per load finishes in a day; half the trucks doing two loads
/// each is cheaper and takes longer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TruckPlan {
    #[default]
    Trucks,
    Trips,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomeLine {
    /// "trips" | "distance" | "helpers" | "dismantle" | "packing" | "survey" | "survey_credit"
    pub key: String,
    pub label: String,
    pub note: String,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomePrice {
    pub lines: Vec<HomeLine>,
    /// The sum of `lines`. Never computed independently.
    pub total_cents: i64,
    pub volume_l: i64,
    pub weight_kg: i64,
    pub item_count: i64,
    pub loads: i64,
    pub trucks: i64,
    pub trips_per_truck: i64,
    /// One driver per truck; the first is the team lead.
    pub drivers: i64,
    pub helpers: i64,
    /// Drivers and helpers: the crew the accepting lead brings.
    pub crew_total: i64,
    pub helper_hours: i64,
    /// Over the Large Estate volume: at least two trucks, and only a
    /// multi-truck capable lead may take it.
    pub large_estate: bool,
    pub survey_required: bool,
    /// The survey fee inside the total: a deposit credited toward the move.
    pub survey_cents: i64,
    pub truck_name: String,
}

fn div_ceil(a: i64, b: i64) -> i64 {
    if b <= 0 { return 0; }
    (a + b - 1) / b
}

/// The whole quote, lines first. `distance_centikm` is the one-way distance
/// in hundredths of a km, so the arithmetic stays in integers.
pub fn price(rates: &HomeRates, property: &Property, items: &[PricedItem], distance_centikm: i64, plan: TruckPlan) -> HomePrice {
    let volume_l: i64 = items.iter().map(|i| i64::from(i.qty) * i64::from(i.volume_l)).sum();
    let weight_kg: i64 = items.iter().map(|i| i64::from(i.qty) * i64::from(i.weight_kg)).sum();
    let item_count: i64 = items.iter().map(|i| i64::from(i.qty)).sum();

    // A load is limited by space or by payload, whichever runs out first.
    let usable_l = rates.truck_volume_l * rates.truck_usable_pct.clamp(1, 100) / 100;
    let loads = div_ceil(volume_l, usable_l).max(div_ceil(weight_kg, rates.truck_payload_kg)).max(1);
    let large_estate = volume_l > rates.large_estate_l;
    let mut trucks = match plan {
        TruckPlan::Trips => div_ceil(loads, 2).max(1),
        TruckPlan::Trucks => loads,
    };
    // A Large Estate is never a one-truck job, whichever plan was asked for.
    if large_estate {
        trucks = trucks.max(2);
    }
    let trips_per_truck = div_ceil(loads, trucks);

    // The size decides the crew; a second truck needs its own pair of hands,
    // so every truck carries at least two helpers once there is more than one.
    let heavy = items.iter().any(|i| i.weight_kg >= 80);
    let base_helpers = if trucks > 1 { property.size_helpers().max(2 * trucks) } else { property.size_helpers() };
    let helpers = base_helpers + i64::from(property.walk_up()) + i64::from(heavy) + i64::from(property.long_carry);
    let helper_hours = rates.min_helper_hours.max(div_ceil(volume_l, rates.m3_per_helper_hour.max(1) * 1000));

    let dismantle: i64 = items.iter().filter(|i| i.dismantle).map(|i| i64::from(i.qty)).sum();
    let packing: i64 = items.iter().filter(|i| i.packing).map(|i| i64::from(i.qty)).sum();
    let survey_required = property.survey_required();

    // Each load is a round trip over the route, so distance is charged both
    // ways. Rounded once, per load, half up.
    let per_load_distance = (distance_centikm.max(0) * 2 * rates.per_km_cents + 50) / 100;
    let km = distance_centikm as f64 / 100.0;

    let mut lines = vec![
        HomeLine {
            key: "trips".into(),
            label: "Truck trips".into(),
            note: format!("{loads} × {}", rates.truck_name),
            amount_cents: loads * rates.trip_cents,
        },
        HomeLine {
            key: "distance".into(),
            label: "Distance".into(),
            note: format!("{km:.1} km, both ways per load"),
            amount_cents: loads * per_load_distance,
        },
        HomeLine {
            key: "helpers".into(),
            label: "Helpers".into(),
            note: format!("{helpers} × {helper_hours} h"),
            amount_cents: helpers * helper_hours * rates.helper_hour_cents,
        },
    ];
    if dismantle > 0 {
        lines.push(HomeLine {
            key: "dismantle".into(),
            label: "Dismantle and rebuild".into(),
            note: format!("{dismantle} item{}", if dismantle == 1 { "" } else { "s" }),
            amount_cents: dismantle * rates.assembly_cents,
        });
    }
    if packing > 0 {
        lines.push(HomeLine {
            key: "packing".into(),
            label: "Special packing".into(),
            note: format!("{packing} item{}", if packing == 1 { "" } else { "s" }),
            amount_cents: packing * rates.packing_cents,
        });
    }
    if survey_required && rates.survey_cents > 0 {
        lines.push(HomeLine {
            key: "survey".into(),
            label: "Survey".into(),
            note: "An attended survey before the move".into(),
            amount_cents: rates.survey_cents,
        });
        lines.push(HomeLine {
            key: "survey_credit".into(),
            label: "Survey credit".into(),
            note: "A deposit, credited toward the move".into(),
            amount_cents: -rates.survey_cents,
        });
    }
    let total_cents = lines.iter().map(|l| l.amount_cents).sum();

    HomePrice {
        lines,
        total_cents,
        volume_l,
        weight_kg,
        item_count,
        loads,
        trucks,
        trips_per_truck,
        drivers: trucks,
        helpers,
        crew_total: trucks + helpers,
        helper_hours,
        large_estate,
        survey_required,
        survey_cents: if survey_required { rates.survey_cents.max(0) } else { 0 },
        truck_name: rates.truck_name.clone(),
    }
}

// ── The survey's addendum ────────────────────────────────────────────────────

/// Materials and resources the surveyor adds, priced by them and approved by
/// the customer: packing boxes, a hoist, an extra day of a helper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurveyExtra {
    /// "material" | "resource"
    pub kind: String,
    pub name: String,
    pub qty: u32,
    pub unit_cents: i64,
}

pub const MAX_EXTRAS: usize = 50;
/// The most one extra line may cost per unit, in minor units.
pub const MAX_EXTRA_UNIT_CENTS: i64 = 1_000_000;

pub fn validate_extras(extras: &[SurveyExtra]) -> Result<(), String> {
    if extras.len() > MAX_EXTRAS {
        return Err(format!("At most {MAX_EXTRAS} extras"));
    }
    for e in extras {
        if !matches!(e.kind.as_str(), "material" | "resource") {
            return Err(format!("\"{}\" is a material or a resource", e.name));
        }
        if e.name.trim().is_empty() || e.name.len() > 80 {
            return Err("Each extra needs a name of 1–80 characters".into());
        }
        if !(1..=100).contains(&e.qty) {
            return Err(format!("{}: quantity is 1–100", e.name));
        }
        if !(1..=MAX_EXTRA_UNIT_CENTS).contains(&e.unit_cents) {
            return Err(format!("{}: the unit price must be above zero and within the limit", e.name));
        }
    }
    Ok(())
}

/// What the survey's additions cost: the whole job re-priced with them, less
/// what was agreed (never below zero — the price does not go down), plus the
/// extras. Returns the re-priced job and the two amounts.
#[allow(clippy::too_many_arguments)]
pub fn addendum_amount(
    rates: &HomeRates,
    property: &Property,
    booked: &[PricedItem],
    added: &[PricedItem],
    extras: &[SurveyExtra],
    distance_centikm: i64,
    plan: TruckPlan,
    agreed_cents: i64,
) -> (HomePrice, i64, i64) {
    let all: Vec<PricedItem> = booked.iter().chain(added).cloned().collect();
    let repriced = price(rates, property, &all, distance_centikm, plan);
    let items_cents = (repriced.total_cents - agreed_cents).max(0);
    let extras_cents = extras.iter().map(|e| i64::from(e.qty) * e.unit_cents).sum();
    (repriced, items_cents, extras_cents)
}

// ── The survey fee when a move is cancelled ──────────────────────────────────

/// A booked move's survey, as a cancellation needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeSurvey {
    pub survey_at: Option<DateTime<Utc>>,
    pub survey_cents: i64,
    pub submitted: bool,
}

/// How much of the survey deposit a cancellation keeps. Nothing more than
/// the refund cutoff before the survey; all of it once inside the cutoff,
/// once the survey has been done, or on a move that had no survey.
pub fn survey_retained_cents(
    survey_cents: i64,
    survey_at: Option<DateTime<Utc>>,
    survey_done: bool,
    now: DateTime<Utc>,
    refund_until_hours: i64,
) -> i64 {
    let Some(survey_at) = survey_at else { return 0 };
    if survey_cents <= 0 {
        return 0;
    }
    if survey_done || now >= survey_at - Duration::hours(refund_until_hours.max(0)) {
        survey_cents
    } else {
        0
    }
}

/// The share of the captured total a cancellation keeps: the move's own tier
/// rate plus the survey fee kept, never more than the whole. Rounded down,
/// in the customer's favour, as the tier rate is.
pub fn home_retention_bps(tier_bps: i64, survey_retained: i64, total_cents: i64) -> i64 {
    const FULL: i128 = 10_000;
    if total_cents <= 0 {
        return tier_bps.clamp(0, 10_000);
    }
    let survey_bps = (i128::from(survey_retained.max(0)) * FULL / i128::from(total_cents)) as i64;
    (tier_bps.max(0) + survey_bps).clamp(0, 10_000)
}

// ── The calendar ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

/// How many home-move teams a date can take, as driver-ops counts them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct DayCapacity {
    pub date: NaiveDate,
    pub leads: i64,
    pub lead_jobs: i64,
    pub multi_truck_leads: i64,
    pub multi_truck_jobs: i64,
    pub international_leads: i64,
}

/// A move already booked, as the calendar needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookedMove {
    pub move_at: DateTime<Utc>,
    pub survey_at: Option<DateTime<Utc>>,
    pub large_estate: bool,
    pub international: bool,
}

/// Whether a move window still has a team for this move: a lead free in the
/// window and a job left in the day — among multi-truck leads for a Large
/// Estate, among international leads for one abroad.
pub fn move_window_open(
    cap: &DayCapacity,
    booked: &[BookedMove],
    start: DateTime<Utc>,
    large_estate: bool,
    international: bool,
    offset_min: i32,
) -> bool {
    let day = local_today(start, offset_min);
    let same_day: Vec<&BookedMove> = booked.iter().filter(|b| local_today(b.move_at, offset_min) == day).collect();
    let in_window = same_day.iter().filter(|b| b.move_at == start).count() as i64;
    if in_window >= cap.leads || same_day.len() as i64 >= cap.lead_jobs {
        return false;
    }
    if large_estate {
        let large_day = same_day.iter().filter(|b| b.large_estate).count() as i64;
        let large_window = same_day.iter().filter(|b| b.large_estate && b.move_at == start).count() as i64;
        if large_day >= cap.multi_truck_jobs || large_window >= cap.multi_truck_leads {
            return false;
        }
    }
    if international && same_day.iter().filter(|b| b.international).count() as i64 >= cap.international_leads {
        return false;
    }
    true
}

/// Whether a survey window still has a lead to send: the surveyor is the lead.
pub fn survey_window_open(cap: &DayCapacity, booked: &[BookedMove], start: DateTime<Utc>) -> bool {
    (booked.iter().filter(|b| b.survey_at == Some(start)).count() as i64) < cap.leads
}

/// The gap the inventory needs between the survey and the move.
pub const SURVEY_LEAD_DAYS: i64 = 2;

pub fn local_today(now: DateTime<Utc>, offset_min: i32) -> NaiveDate {
    (now + Duration::minutes(i64::from(offset_min))).date_naive()
}

fn at_local(day: NaiveDate, hour: u32, offset_min: i32) -> DateTime<Utc> {
    let local = day.and_time(NaiveTime::from_hms_opt(hour, 0, 0).unwrap_or_default());
    Utc.from_utc_datetime(&local) - Duration::minutes(i64::from(offset_min))
}

fn days(now: DateTime<Utc>, offset_min: i32, from: i64, to: i64) -> impl Iterator<Item = NaiveDate> {
    let today = local_today(now, offset_min);
    (from..=to)
        .map(move |n| today + Duration::days(n))
        .filter(|d| d.weekday() != Weekday::Sun)
}

/// Survey windows: two hours, three a day, from tomorrow for ten days.
/// Sundays are off. There is no crew calendar behind this — every window is
/// offered, and dispatch finds the surveyor.
pub fn survey_slots(now: DateTime<Utc>, offset_min: i32) -> Vec<Slot> {
    days(now, offset_min, 1, 10)
        .flat_map(|d| [8, 10, 13].map(|h| Slot { starts_at: at_local(d, h, offset_min), ends_at: at_local(d, h + 2, offset_min) }))
        .collect()
}

/// Move starts: morning and afternoon, from three days out for three weeks.
pub fn move_slots(now: DateTime<Utc>, offset_min: i32) -> Vec<Slot> {
    days(now, offset_min, 3, 24)
        .flat_map(|d| [8, 13].map(|h| Slot { starts_at: at_local(d, h, offset_min), ends_at: at_local(d, h + 4, offset_min) }))
        .collect()
}

/// The pair a booking asks for, held to the calendar: the move is an offered
/// start; a survey is given exactly when one is required, is an offered
/// window, and lands at least two local days before the move.
pub fn check_schedule(
    survey_at: Option<DateTime<Utc>>,
    move_at: DateTime<Utc>,
    survey_required: bool,
    now: DateTime<Utc>,
    offset_min: i32,
) -> Result<(), String> {
    if !move_slots(now, offset_min).iter().any(|s| s.starts_at == move_at) {
        return Err("That move time isn't one we offer — pick one of the listed times".into());
    }
    match (survey_required, survey_at) {
        (false, None) => Ok(()),
        (false, Some(_)) => Err("This move is not surveyed — leave the survey out".into()),
        (true, None) => Err("This move needs a survey first — pick a survey time".into()),
        (true, Some(survey)) => {
            if !survey_slots(now, offset_min).iter().any(|s| s.starts_at == survey) {
                return Err("That survey time isn't one we offer".into());
            }
            let gap = local_today(move_at, offset_min) - local_today(survey, offset_min);
            if gap.num_days() < SURVEY_LEAD_DAYS {
                return Err("The survey must be at least two days before the move, so the inventory can be corrected".into());
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rates() -> HomeRates {
        HomeRates {
            trip_cents: 22_000,
            per_km_cents: 240,
            helper_hour_cents: 2_800,
            assembly_cents: 2_600,
            packing_cents: 2_500,
            survey_cents: 4_500,
            ..HomeRates::default()
        }
    }

    fn apartment(size: &str) -> Property {
        Property {
            kind: PropertyType::Apartment,
            size: size.into(),
            pickup_floor: 4,
            pickup_has_lift: true,
            dropoff_floor: 2,
            dropoff_has_lift: true,
            long_carry: false,
        }
    }

    fn item(room: &str, key: &str, qty: u32, volume_l: i32, kg: i32) -> PricedItem {
        PricedItem { room: room.into(), item_key: key.into(), name: key.into(), qty, volume_l, weight_kg: kg, dismantle: false, packing: false }
    }

    #[test]
    fn the_survey_is_for_two_bedrooms_up_and_every_villa_and_office() {
        assert!(!apartment("Studio").survey_required());
        assert!(!apartment("1 bedroom").survey_required());
        assert!(apartment("2 bedroom").survey_required());
        assert!(Property { kind: PropertyType::Villa, size: "Studio".into(), ..apartment("Studio") }.survey_required());
        assert!(Property { kind: PropertyType::Offices, size: "Up to 10 desks".into(), ..apartment("Studio") }.survey_required());
    }

    #[test]
    fn a_size_from_another_type_is_refused() {
        assert!(Property { kind: PropertyType::Offices, ..apartment("3 bedroom") }.validate().is_err());
        assert!(Property { pickup_floor: 9, ..apartment("3 bedroom") }.validate().is_err());
    }

    /// The design's own example shape: one load, one truck, two helpers, the
    /// four-hour minimum; every line in integers and the lines summing.
    #[test]
    fn a_small_home_prices_line_by_line_and_the_lines_sum() {
        let items = vec![item("living", "sofa_3_seat", 1, 2_100, 78), item("living", "packed_box", 10, 200, 18)];
        let p = price(&rates(), &apartment("1 bedroom"), &items, 1_840, TruckPlan::Trucks);
        assert_eq!((p.volume_l, p.weight_kg, p.item_count), (4_100, 258, 11));
        assert_eq!((p.loads, p.trucks, p.trips_per_truck, p.helpers, p.helper_hours), (1, 1, 1, 2, 4));
        let by = |k: &str| p.lines.iter().find(|l| l.key == k).map(|l| l.amount_cents);
        assert_eq!(by("trips"), Some(22_000));
        // 18.40 km × 2 × 2.40 = 88.32
        assert_eq!(by("distance"), Some(8_832));
        assert_eq!(by("helpers"), Some(2 * 4 * 2_800));
        assert_eq!(by("survey"), None, "a one-bedroom apartment is self-declared");
        assert_eq!(p.total_cents, p.lines.iter().map(|l| l.amount_cents).sum::<i64>());
    }

    #[test]
    fn loads_follow_space_or_payload_and_the_plan_halves_the_trucks() {
        // 40 m³ over 15.3 m³ usable a truck = 3 loads.
        let big = vec![item("living", "box", 200, 200, 5)];
        let p = price(&rates(), &apartment("3 bedroom"), &big, 1_000, TruckPlan::Trucks);
        assert_eq!((p.loads, p.trucks), (3, 3));
        let q = price(&rates(), &apartment("3 bedroom"), &big, 1_000, TruckPlan::Trips);
        assert_eq!((q.loads, q.trucks, q.trips_per_truck), (3, 2, 2));
        // Tiny but heavy: payload decides.
        let heavy = vec![item("garage", "safe", 7, 100, 500)];
        assert_eq!(price(&rates(), &apartment("1 bedroom"), &heavy, 1_000, TruckPlan::Trucks).loads, 2);
    }

    #[test]
    fn a_walk_up_a_heavy_item_and_a_long_carry_each_add_a_helper() {
        let prop = Property { pickup_has_lift: false, long_carry: true, ..apartment("2 bedroom") };
        let items = vec![item("kitchen", "fridge", 1, 1_100, 116)];
        // A two-bedroom's three, then one each for the walk-up, the heavy
        // fridge and the long carry.
        assert_eq!(price(&rates(), &prop, &items, 500, TruckPlan::Trucks).helpers, 3 + 1 + 1 + 1);
    }

    #[test]
    fn the_crew_follows_the_size_table() {
        let one_box = vec![item("living", "box", 1, 200, 5)];
        for (size, helpers) in [("Studio", 1), ("1 bedroom", 2), ("2 bedroom", 3), ("3 bedroom", 4), ("4 bedroom", 5)] {
            let p = price(&rates(), &apartment(size), &one_box, 500, TruckPlan::Trucks);
            assert_eq!((p.drivers, p.helpers, p.crew_total), (1, helpers, 1 + helpers), "{size}");
        }
    }

    #[test]
    fn more_than_one_truck_carries_two_helpers_each() {
        // 20 m³: two loads, two trucks. A studio's one helper becomes four.
        let items = vec![item("living", "box", 100, 200, 5)];
        let p = price(&rates(), &apartment("Studio"), &items, 500, TruckPlan::Trucks);
        assert_eq!((p.trucks, p.helpers, p.crew_total), (2, 4, 6));
    }

    #[test]
    fn a_large_estate_is_never_one_truck() {
        let small_trucks = HomeRates { truck_volume_l: 100_000, ..rates() };
        // 80 m³ fits one very large truck, but it is over the 75 m³ line.
        let items = vec![item("living", "box", 400, 200, 5)];
        let p = price(&small_trucks, &apartment("5 bedroom +"), &items, 500, TruckPlan::Trips);
        assert!(p.large_estate);
        assert_eq!(p.trucks, 2);
        // Five bedrooms and up bring six, already over two a truck.
        assert_eq!(p.helpers, 6);
        let q = price(&small_trucks, &apartment("5 bedroom +"), &[item("living", "box", 10, 200, 5)], 500, TruckPlan::Trips);
        assert!(!q.large_estate);
        assert_eq!(q.trucks, 1);
    }

    #[test]
    fn the_survey_fee_is_refunded_only_well_before_the_survey() {
        let survey = Utc::now() + Duration::hours(20);
        assert_eq!(survey_retained_cents(4_500, Some(survey), false, survey - Duration::hours(13), 12), 0);
        assert_eq!(survey_retained_cents(4_500, Some(survey), false, survey - Duration::hours(11), 12), 4_500);
        assert_eq!(survey_retained_cents(4_500, Some(survey), true, survey - Duration::hours(40), 12), 4_500, "done is kept");
        assert_eq!(survey_retained_cents(4_500, None, false, survey, 12), 0, "no survey, no fee");
    }

    #[test]
    fn retention_adds_the_survey_fee_to_the_tier_and_never_exceeds_the_whole() {
        // 45.00 of 1,500.00 is 300 bps; with a 15% tier, 1,800.
        assert_eq!(home_retention_bps(1_500, 4_500, 150_000), 1_800);
        assert_eq!(home_retention_bps(0, 4_500, 150_000), 300);
        assert_eq!(home_retention_bps(9_900, 4_500, 150_000), 10_000);
        // Rounded down: 1.00 of 3.00 is 3,333.3 bps.
        assert_eq!(home_retention_bps(0, 100, 300), 3_333);
    }

    #[test]
    fn an_addendum_charges_the_increase_and_never_lowers_the_price() {
        let booked = vec![item("living", "sofa", 1, 2_100, 78)];
        let prop = apartment("3 bedroom");
        let agreed = price(&rates(), &prop, &booked, 1_000, TruckPlan::Trucks).total_cents;

        // Nothing found: nothing owed.
        let (_, items, extras) = addendum_amount(&rates(), &prop, &booked, &[], &[], 1_000, TruckPlan::Trucks, agreed);
        assert_eq!((items, extras), (0, 0));

        // Twenty boxes more: the re-priced job less what was agreed.
        let boxes = vec![item("storage", "box", 20, 200, 18)];
        let (repriced, items, _) = addendum_amount(&rates(), &prop, &booked, &boxes, &[], 1_000, TruckPlan::Trucks, agreed);
        assert_eq!(items, repriced.total_cents - agreed);

        // Rates dropped since booking: still nothing negative.
        let cheaper = HomeRates { trip_cents: 1, helper_hour_cents: 1, ..rates() };
        let (_, items, _) = addendum_amount(&cheaper, &prop, &booked, &boxes, &[], 1_000, TruckPlan::Trucks, agreed);
        assert_eq!(items, 0);

        let wrap = SurveyExtra { kind: "material".into(), name: "Bubble wrap roll".into(), qty: 3, unit_cents: 450 };
        let (_, _, extras) = addendum_amount(&rates(), &prop, &booked, &[], std::slice::from_ref(&wrap), 1_000, TruckPlan::Trucks, agreed);
        assert_eq!(extras, 1_350);
    }

    #[test]
    fn extras_are_named_bounded_and_priced() {
        let ok = SurveyExtra { kind: "resource".into(), name: "Hoist".into(), qty: 1, unit_cents: 50_000 };
        assert!(validate_extras(std::slice::from_ref(&ok)).is_ok());
        assert!(validate_extras(&[SurveyExtra { kind: "gift".into(), ..ok.clone() }]).is_err());
        assert!(validate_extras(&[SurveyExtra { unit_cents: 0, ..ok.clone() }]).is_err());
        assert!(validate_extras(&[SurveyExtra { qty: 0, ..ok.clone() }]).is_err());
        assert!(validate_extras(&[SurveyExtra { name: " ".into(), ..ok }]).is_err());
    }

    #[test]
    fn the_survey_fee_and_its_credit_net_to_nothing() {
        let items = vec![item("living", "sofa", 1, 2_100, 78)];
        let p = price(&rates(), &apartment("3 bedroom"), &items, 1_000, TruckPlan::Trucks);
        let survey: i64 = p.lines.iter().filter(|l| l.key.starts_with("survey")).map(|l| l.amount_cents).sum();
        assert_eq!(survey, 0);
        assert_eq!(p.lines.iter().filter(|l| l.key.starts_with("survey")).count(), 2);
    }

    #[test]
    fn declared_lines_resolve_against_the_catalogue_of_their_room() {
        let catalogue = vec![CatalogueItem {
            key: "king_bed".into(), group: "bed".into(), name: "King bed".into(),
            volume_l: 2_600, weight_kg: 96, assembly: true, packing: false,
        }];
        let ok = [DeclaredItem { room: "master".into(), item_key: "king_bed".into(), qty: 1, dismantle: true, packing: false }];
        assert_eq!(resolve(&apartment("3 bedroom"), &ok, &catalogue).unwrap()[0].volume_l, 2_600);

        let wrong_room = [DeclaredItem { room: "kitchen".into(), ..ok[0].clone() }];
        assert!(resolve(&apartment("3 bedroom"), &wrong_room, &catalogue).is_err());
        let office_room = [DeclaredItem { room: "server".into(), ..ok[0].clone() }];
        assert!(resolve(&apartment("3 bedroom"), &office_room, &catalogue).is_err());
        let zero = [DeclaredItem { qty: 0, ..ok[0].clone() }];
        assert!(resolve(&apartment("3 bedroom"), &zero, &catalogue).is_err());
        assert!(resolve(&apartment("3 bedroom"), &[], &catalogue).is_err());
    }

    fn now() -> DateTime<Utc> {
        // Thu 17 Sep 2026, 10:00 Manila.
        Utc.with_ymd_and_hms(2026, 9, 17, 2, 0, 0).unwrap()
    }

    #[test]
    fn the_calendar_skips_sundays_and_starts_where_it_says() {
        let surveys = survey_slots(now(), 480);
        assert!(surveys.iter().all(|s| (s.starts_at + Duration::minutes(480)).weekday() != Weekday::Sun));
        // First survey: Fri 18 Sep 08:00 Manila = 00:00 UTC.
        assert_eq!(surveys[0].starts_at, Utc.with_ymd_and_hms(2026, 9, 18, 0, 0, 0).unwrap());
        let moves = move_slots(now(), 480);
        // First move: Sun 20 is skipped → Mon 21 08:00 Manila.
        assert_eq!(moves[0].starts_at, Utc.with_ymd_and_hms(2026, 9, 21, 0, 0, 0).unwrap());
    }

    fn cap(leads: i64, jobs: i64, multi: i64, intl: i64) -> DayCapacity {
        DayCapacity {
            date: NaiveDate::from_ymd_opt(2026, 9, 21).unwrap(),
            leads,
            lead_jobs: jobs,
            multi_truck_leads: multi,
            multi_truck_jobs: multi,
            international_leads: intl,
        }
    }

    fn booked(at: DateTime<Utc>, large: bool) -> BookedMove {
        BookedMove { move_at: at, survey_at: None, large_estate: large, international: false }
    }

    #[test]
    fn a_window_closes_when_its_teams_or_the_days_jobs_run_out() {
        let morning = Utc.with_ymd_and_hms(2026, 9, 21, 0, 0, 0).unwrap(); // 08:00 Manila
        let afternoon = morning + Duration::hours(5);
        // Two leads, one job each.
        let two = cap(2, 2, 0, 0);
        assert!(move_window_open(&two, &[], morning, false, false, 480));
        assert!(move_window_open(&two, &[booked(morning, false)], morning, false, false, 480));
        assert!(!move_window_open(&two, &[booked(morning, false), booked(morning, false)], morning, false, false, 480));
        // One lead with two jobs a day: morning taken, afternoon still open.
        let one = cap(1, 2, 0, 0);
        assert!(!move_window_open(&one, &[booked(morning, false)], morning, false, false, 480));
        assert!(move_window_open(&one, &[booked(morning, false)], afternoon, false, false, 480));
        // Jobs for the day spent: every window closed.
        assert!(!move_window_open(&cap(3, 1, 0, 0), &[booked(morning, false)], afternoon, false, false, 480));
        // Nobody working.
        assert!(!move_window_open(&DayCapacity::default(), &[], morning, false, false, 480));
    }

    #[test]
    fn a_large_estate_needs_a_free_multi_truck_lead_and_abroad_an_international_one() {
        let morning = Utc.with_ymd_and_hms(2026, 9, 21, 0, 0, 0).unwrap();
        assert!(!move_window_open(&cap(3, 3, 0, 0), &[], morning, true, false, 480));
        assert!(move_window_open(&cap(3, 3, 1, 0), &[], morning, true, false, 480));
        assert!(!move_window_open(&cap(3, 3, 1, 0), &[booked(morning + Duration::hours(5), true)], morning, true, false, 480));
        assert!(!move_window_open(&cap(3, 3, 0, 0), &[], morning, false, true, 480));
    }

    #[test]
    fn a_survey_window_closes_when_every_lead_is_surveying_in_it() {
        let at = Utc.with_ymd_and_hms(2026, 9, 18, 0, 0, 0).unwrap();
        let surveyed = BookedMove { move_at: at + Duration::days(3), survey_at: Some(at), large_estate: false, international: false };
        assert!(survey_window_open(&cap(2, 2, 0, 0), std::slice::from_ref(&surveyed), at));
        assert!(!survey_window_open(&cap(1, 2, 0, 0), &[surveyed], at));
    }

    #[test]
    fn the_schedule_holds_the_survey_two_days_ahead() {
        let surveys = survey_slots(now(), 480);
        let moves = move_slots(now(), 480);
        let fri_survey = surveys[0].starts_at; // Fri 18
        let mon_move = moves[0].starts_at; // Mon 21
        assert!(check_schedule(Some(fri_survey), mon_move, true, now(), 480).is_ok());

        let sat_survey = surveys.iter().find(|s| (s.starts_at + Duration::minutes(480)).day() == 19).unwrap().starts_at;
        let mon_too_soon = check_schedule(Some(sat_survey), mon_move, true, now(), 480);
        assert!(mon_too_soon.is_ok(), "Sat → Mon is two days");
        let tue_survey = surveys.iter().find(|s| (s.starts_at + Duration::minutes(480)).day() == 22).unwrap().starts_at;
        assert!(check_schedule(Some(tue_survey), mon_move, true, now(), 480).is_err());

        assert!(check_schedule(None, mon_move, true, now(), 480).is_err(), "a surveyed move needs its survey");
        assert!(check_schedule(Some(fri_survey), mon_move, false, now(), 480).is_err(), "and a self-declared one has none");
        assert!(check_schedule(None, mon_move + Duration::minutes(30), false, now(), 480).is_err(), "only offered starts");
    }
}

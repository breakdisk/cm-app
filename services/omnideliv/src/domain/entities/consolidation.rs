//! Multi-stop consolidation.
//!
//! Consolidation is the margin lever, not a customer perk: the fee is flat
//! regardless of stop count, while courier cost barely rises with a second
//! pickup and each additional vendor adds a full commission leg. Sequencing
//! quality is therefore revenue, not decoration.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemperatureClass {
    Hot,
    Chilled,
    Ambient,
}

impl TemperatureClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            TemperatureClass::Hot     => "hot",
            TemperatureClass::Chilled => "chilled",
            TemperatureClass::Ambient => "ambient",
        }
    }
}

/// A stop before sequencing.
#[derive(Debug, Clone)]
pub struct PendingStop {
    pub vendor_id:         Uuid,
    pub prep_time_minutes: i32,
    pub temperature_class: TemperatureClass,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Stop {
    pub vendor_id:         Uuid,
    pub seq:               i32,
    pub prep_time_minutes: i32,
    pub temperature_class: TemperatureClass,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationPlan {
    pub id:                  Uuid,
    pub tenant_id:           Uuid,
    pub basket_id:           Uuid,
    pub stops:               Vec<Stop>,
    pub total_distance_m:    i32,
    pub flat_fee_cents:      i64,
    pub temperature_classes: Vec<TemperatureClass>,
    pub created_at:          DateTime<Utc>,
}

impl ConsolidationPlan {
    /// Sequence stops by readiness, soonest first.
    ///
    /// Not by distance. A grocery pick ready in 5 minutes collected before a
    /// kitchen order ready in 20 means the hot food is the last thing in the bag
    /// and the first thing out — the difference between a warm meal and a
    /// refund. Distance still shapes the fee via total_distance_m; it just does
    /// not decide the order.
    ///
    /// Ties break on vendor id so the sequence is deterministic — a route that
    /// reorders between two identical calls would make dispatch untestable and
    /// would silently reshuffle a courier's stops on a retry.
    pub fn sequence(
        tenant_id: Uuid,
        basket_id: Uuid,
        mut pending: Vec<PendingStop>,
        total_distance_m: i32,
        flat_fee_cents: i64,
    ) -> Self {
        pending.sort_by(|a, b| {
            a.prep_time_minutes
                .cmp(&b.prep_time_minutes)
                .then_with(|| a.vendor_id.cmp(&b.vendor_id))
        });

        let stops: Vec<Stop> = pending
            .iter()
            .enumerate()
            .map(|(i, p)| Stop {
                vendor_id:         p.vendor_id,
                seq:               i as i32,
                prep_time_minutes: p.prep_time_minutes,
                temperature_class: p.temperature_class,
            })
            .collect();

        // Distinct classes, in a stable order so the value is comparable
        // between runs and readable in the ops UI.
        let mut classes: Vec<TemperatureClass> =
            pending.iter().map(|p| p.temperature_class).collect();
        classes.sort_by_key(|c| match c {
            TemperatureClass::Hot     => 0,
            TemperatureClass::Chilled => 1,
            TemperatureClass::Ambient => 2,
        });
        classes.dedup();

        Self {
            id: Uuid::new_v4(),
            tenant_id,
            basket_id,
            stops,
            total_distance_m,
            flat_fee_cents,
            temperature_classes: classes,
            created_at: Utc::now(),
        }
    }

    /// True when the basket spans more than one temperature class — the
    /// cross-category constraint Screen B surfaces.
    pub fn has_mixed_temperatures(&self) -> bool {
        self.temperature_classes.len() > 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn stop(prep_mins: i32, class: TemperatureClass) -> PendingStop {
        PendingStop {
            vendor_id: Uuid::new_v4(),
            prep_time_minutes: prep_mins,
            temperature_class: class,
        }
    }

    /// The sequencing rule: collect what is ready soonest first, so the hot
    /// items spend the least time in the bag.
    #[test]
    fn stops_are_sequenced_by_readiness_not_input_order() {
        let kitchen = stop(20, TemperatureClass::Hot);
        let grocery = stop(5, TemperatureClass::Chilled);

        // Deliberately pass the slow stop first.
        let plan = ConsolidationPlan::sequence(
            Uuid::new_v4(), Uuid::new_v4(), vec![kitchen.clone(), grocery.clone()], 4_200, 7_900,
        );

        assert_eq!(plan.stops[0].vendor_id, grocery.vendor_id, "the 5-minute pick goes first");
        assert_eq!(plan.stops[1].vendor_id, kitchen.vendor_id, "the 20-minute kitchen goes last");
    }

    #[test]
    fn a_single_stop_route_is_trivially_sequenced() {
        let only = stop(15, TemperatureClass::Hot);
        let plan = ConsolidationPlan::sequence(Uuid::new_v4(), Uuid::new_v4(), vec![only.clone()], 1_100, 4_900);
        assert_eq!(plan.stops.len(), 1);
        assert_eq!(plan.stops[0].seq, 0);
    }

    /// A mixed-temperature basket is flagged so ops can see why the route was
    /// ordered the way it was — and so Screen B can show the constraint.
    #[test]
    fn a_mixed_temperature_basket_records_both_classes() {
        let plan = ConsolidationPlan::sequence(
            Uuid::new_v4(), Uuid::new_v4(),
            vec![stop(20, TemperatureClass::Hot), stop(5, TemperatureClass::Chilled)],
            4_200, 7_900,
        );
        assert_eq!(plan.temperature_classes.len(), 2);
        assert!(plan.temperature_classes.contains(&TemperatureClass::Hot));
        assert!(plan.temperature_classes.contains(&TemperatureClass::Chilled));
        assert!(plan.has_mixed_temperatures());
    }

    #[test]
    fn a_single_class_basket_records_one_class() {
        let plan = ConsolidationPlan::sequence(
            Uuid::new_v4(), Uuid::new_v4(),
            vec![stop(10, TemperatureClass::Ambient), stop(5, TemperatureClass::Ambient)],
            2_000, 5_900,
        );
        assert_eq!(plan.temperature_classes, vec![TemperatureClass::Ambient]);
        assert!(!plan.has_mixed_temperatures());
    }

    /// THE PRODUCT PROMISE: one fee, whatever the stop count. A per-stop fee
    /// would make consolidation a cost to the customer instead of a benefit.
    #[test]
    fn the_fee_is_flat_regardless_of_stop_count() {
        let one = ConsolidationPlan::sequence(
            Uuid::new_v4(), Uuid::new_v4(), vec![stop(10, TemperatureClass::Hot)], 3_000, 7_900);
        let three = ConsolidationPlan::sequence(
            Uuid::new_v4(), Uuid::new_v4(),
            vec![stop(10, TemperatureClass::Hot), stop(5, TemperatureClass::Chilled), stop(8, TemperatureClass::Ambient)],
            3_000, 7_900,
        );
        assert_eq!(one.flat_fee_cents, three.flat_fee_cents);
    }

    #[test]
    fn seq_numbers_are_contiguous_from_zero() {
        let plan = ConsolidationPlan::sequence(
            Uuid::new_v4(), Uuid::new_v4(),
            vec![stop(30, TemperatureClass::Hot), stop(5, TemperatureClass::Chilled), stop(15, TemperatureClass::Ambient)],
            5_000, 8_900,
        );
        let seqs: Vec<i32> = plan.stops.iter().map(|s| s.seq).collect();
        assert_eq!(seqs, vec![0, 1, 2]);
    }

    /// Sequencing must be deterministic: two inputs differing only in order must
    /// produce the same route, or dispatch cannot be tested and a retried plan
    /// silently reshuffles a courier's stops.
    #[test]
    fn equal_prep_times_break_ties_deterministically() {
        let a = stop(10, TemperatureClass::Ambient);
        let b = stop(10, TemperatureClass::Ambient);

        let forward = ConsolidationPlan::sequence(
            Uuid::new_v4(), Uuid::new_v4(), vec![a.clone(), b.clone()], 1_000, 4_900);
        let reversed = ConsolidationPlan::sequence(
            Uuid::new_v4(), Uuid::new_v4(), vec![b.clone(), a.clone()], 1_000, 4_900);

        assert_eq!(
            forward.stops.iter().map(|s| s.vendor_id).collect::<Vec<_>>(),
            reversed.stops.iter().map(|s| s.vendor_id).collect::<Vec<_>>(),
        );
    }
}

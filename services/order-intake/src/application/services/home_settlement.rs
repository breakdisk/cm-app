//! Pay for a whole-home move that is not the primary lead's task payout,
//! credited when the move is delivered:
//! - a Joint Mission's Support Lead: their 40% of the lead's pay;
//! - each paid addendum: its fare less commission, to the move's own lead —
//!   shared with the extra truck's lead where a major overflow brought one.
//!
//! Every credit is idempotent in driver-ops (once per lead, kind and
//! reference), so a redelivered event pays nothing twice.

use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::value_objects::home_move::{addendum_split, joint_split, share_of, HomeRates, LeadPay};
use crate::infrastructure::db::{home_addenda, home_moves};
use crate::infrastructure::http::home_capacity_client::{HomeTeamsClient, LeadCredit, SlotLead};

/// A credit to make: to whom, for what, how much.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Due {
    pub driver_id: Uuid,
    pub kind: &'static str,
    pub reference_id: Uuid,
    pub pay: LeadPay,
}

/// An addendum as the split needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaidAddendum {
    pub id: Uuid,
    pub total_cents: i64,
    /// Extra trucks it needed beyond those booked before it.
    pub trucks_needed: i64,
}

/// Who is owed what beyond the primary's task payout. Pure: the move's pay,
/// its paid addenda (in the order paid), and the leads on it.
pub fn dues(
    rates: &HomeRates,
    shipment_id: Uuid,
    lead_pay: LeadPay,
    joint: bool,
    addenda: &[PaidAddendum],
    slots: &[SlotLead],
) -> Vec<Due> {
    let mut out = Vec::new();
    let primary = slots.iter().find(|s| s.role == "sole" || s.role == "captain");
    if joint {
        if let Some(support) = slots.iter().find(|s| s.role == "support") {
            let (_, support_net) = joint_split(lead_pay.net_cents);
            out.push(Due { driver_id: support.driver_id, kind: "support_share", reference_id: shipment_id, pay: share_of(lead_pay, support_net) });
        }
    }
    // Extra trucks, in the order they were taken, go to the addenda that needed them.
    let mut extra = slots.iter().filter(|s| s.role == "emergency");
    for a in addenda {
        let came: Vec<&SlotLead> = extra.by_ref().take(usize::try_from(a.trucks_needed.max(0)).unwrap_or(0)).collect();
        let (to_primary, per_truck) = addendum_split(rates, a.total_cents, a.trucks_needed, came.len() as i64);
        if let Some(p) = primary {
            if to_primary.net_cents > 0 {
                out.push(Due { driver_id: p.driver_id, kind: "addendum_share", reference_id: a.id, pay: to_primary });
            }
        }
        for lead in came {
            out.push(Due { driver_id: lead.driver_id, kind: "emergency_share", reference_id: a.id, pay: per_truck });
        }
    }
    out
}

/// The move was delivered: credit everyone owed beyond the primary's task.
/// Errors are the caller's to log; a later redelivery retries safely.
pub async fn credit_on_delivery(pool: &PgPool, teams: &HomeTeamsClient, rates: &HomeRates, shipment_id: Uuid) -> anyhow::Result<usize> {
    let Some(record) = home_moves::by_shipment(pool, shipment_id).await? else {
        return Ok(0); // not a home move
    };
    let Some(reservation) = teams.reservation(shipment_id).await? else {
        tracing::warn!(%shipment_id, "home move delivered with no reserved lead — no shares credited");
        return Ok(0);
    };
    let addenda: Vec<PaidAddendum> = home_addenda::paid_for(pool, shipment_id)
        .await?
        .iter()
        .map(|a| PaidAddendum {
            id: a.id,
            total_cents: a.total_cents,
            trucks_needed: a.trucks_before.map_or(0, |before| i64::from((a.trucks - before).max(0))),
        })
        .collect();
    let lead_pay = LeadPay {
        gross_cents: record.lead_gross_cents + record.lead_bonus_cents,
        commission_cents: record.lead_commission_cents,
        net_cents: record.lead_payout_cents,
    };
    let joint = reservation.role == "captain";
    let tracking: Option<String> = sqlx::query_scalar("SELECT awb FROM order_intake.shipments WHERE id = $1")
        .bind(shipment_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    let due = dues(rates, shipment_id, lead_pay, joint, &addenda, &reservation.slots);
    for d in &due {
        teams
            .credit(&LeadCredit {
                tenant_id: record.tenant_id,
                driver_id: d.driver_id,
                kind: d.kind,
                reference_id: d.reference_id,
                tracking_number: tracking.clone(),
                gross_cents: d.pay.gross_cents,
                commission_cents: d.pay.commission_cents,
                amount_cents: d.pay.net_cents,
            })
            .await?;
    }
    if !due.is_empty() {
        tracing::info!(%shipment_id, credits = due.len(), "home move delivered: shares credited");
    }
    Ok(due.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lead(role: &str) -> SlotLead {
        SlotLead { driver_id: Uuid::new_v4(), lead_name: String::new(), role: role.into(), payout_cents: None }
    }

    fn pay() -> LeadPay {
        LeadPay { gross_cents: 125_000, commission_cents: 25_000, net_cents: 100_000 }
    }

    #[test]
    fn a_sole_move_with_no_addendum_owes_nothing_beyond_the_task() {
        assert!(dues(&HomeRates::default(), Uuid::nil(), pay(), false, &[], &[lead("sole")]).is_empty());
    }

    #[test]
    fn the_support_lead_is_paid_forty_percent_of_a_joint_mission() {
        let slots = [lead("captain"), lead("support")];
        let due = dues(&HomeRates::default(), Uuid::nil(), pay(), true, &[], &slots);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].driver_id, slots[1].driver_id);
        assert_eq!(due[0].kind, "support_share");
        assert_eq!(due[0].pay, LeadPay { gross_cents: 50_000, commission_cents: 10_000, net_cents: 40_000 });
    }

    #[test]
    fn a_minor_overflow_addendum_is_all_the_leads() {
        let slots = [lead("sole")];
        let a = PaidAddendum { id: Uuid::new_v4(), total_cents: 500_000, trucks_needed: 0 };
        let due = dues(&HomeRates::default(), Uuid::nil(), pay(), false, &[a], &slots);
        assert_eq!(due.len(), 1);
        assert_eq!((due[0].kind, due[0].pay.net_cents, due[0].reference_id), ("addendum_share", 400_000, a.id));
    }

    #[test]
    fn a_major_overflow_shares_the_addendum_with_the_truck_that_came() {
        let slots = [lead("sole"), lead("emergency")];
        let a = PaidAddendum { id: Uuid::new_v4(), total_cents: 1_000_000, trucks_needed: 1 };
        let due = dues(&HomeRates::default(), Uuid::nil(), pay(), false, &[a], &slots);
        assert_eq!(due.len(), 2);
        let extra = due.iter().find(|d| d.kind == "emergency_share").expect("emergency share");
        assert_eq!(extra.driver_id, slots[1].driver_id);
        assert_eq!(extra.pay.net_cents, 400_000);
        let own = due.iter().find(|d| d.kind == "addendum_share").expect("lead's share");
        assert_eq!(own.pay.net_cents, 400_000);
    }

    #[test]
    fn if_the_extra_truck_never_came_the_lead_keeps_the_addendum() {
        let slots = [lead("sole")];
        let a = PaidAddendum { id: Uuid::new_v4(), total_cents: 1_000_000, trucks_needed: 1 };
        let due = dues(&HomeRates::default(), Uuid::nil(), pay(), false, &[a], &slots);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].pay.net_cents, 800_000);
    }
}

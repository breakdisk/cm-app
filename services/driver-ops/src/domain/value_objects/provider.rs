//! A provider's profile: which jobs a driver is onboarded for, and when they
//! work. The onboarding filter decided 2026-09-19: freight moves and whole-home
//! moves are separate service lines, a home move needs a lead with that niche,
//! and coverage is local or international.
//!
//! Capabilities are set by operations at onboarding; working days and off days
//! are the lead's own. Pure: no clock, no database.

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

pub const SERVICE_LINES: [&str; 2] = ["freight_move", "home_move"];
pub const COVERAGE: [&str; 2] = ["local", "international"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderProfile {
    pub service_lines: Vec<String>,
    pub coverage: Vec<String>,
    /// Model A: an Enterprise Team Lead who brings more than one truck.
    pub multi_truck_capable: bool,
    pub fleet_trucks: i32,
    /// Helpers the lead employs and brings.
    pub registered_helpers: i32,
    pub max_daily_jobs: i32,
    /// 0 = Monday … 6 = Sunday.
    pub working_days: Vec<i16>,
}

impl Default for ProviderProfile {
    /// Every driver onboarded before this: freight, local, one truck, no
    /// crew, one job a day, Monday to Saturday.
    fn default() -> Self {
        Self {
            service_lines: vec!["freight_move".into()],
            coverage: vec!["local".into()],
            multi_truck_capable: false,
            fleet_trucks: 1,
            registered_helpers: 0,
            max_daily_jobs: 1,
            working_days: vec![0, 1, 2, 3, 4, 5],
        }
    }
}

fn subset(values: &[String], allowed: &[&str], what: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = values.iter().map(|v| v.trim().to_ascii_lowercase()).collect();
    out.sort();
    out.dedup();
    if out.is_empty() {
        return Err(format!("At least one {what}"));
    }
    if let Some(bad) = out.iter().find(|v| !allowed.contains(&v.as_str())) {
        return Err(format!("\"{bad}\" is not a {what} ({})", allowed.join(", ")));
    }
    Ok(out)
}

pub fn working_days(days: &[i16]) -> Result<Vec<i16>, String> {
    let mut out = days.to_vec();
    out.sort_unstable();
    out.dedup();
    if out.iter().any(|d| !(0..=6).contains(d)) {
        return Err("Working days run 0 (Monday) to 6 (Sunday)".into());
    }
    Ok(out)
}

impl ProviderProfile {
    /// The profile as stored: lists normalised, every bound held.
    pub fn validated(mut self) -> Result<Self, String> {
        self.service_lines = subset(&self.service_lines, &SERVICE_LINES, "service line")?;
        self.coverage = subset(&self.coverage, &COVERAGE, "coverage")?;
        self.working_days = working_days(&self.working_days)?;
        if !(1..=20).contains(&self.fleet_trucks) {
            return Err("Trucks run 1 to 20".into());
        }
        if !(0..=60).contains(&self.registered_helpers) {
            return Err("Registered helpers run 0 to 60".into());
        }
        if !(1..=4).contains(&self.max_daily_jobs) {
            return Err("Jobs a day run 1 to 4".into());
        }
        if self.multi_truck_capable && self.fleet_trucks < 2 {
            return Err("A multi-truck lead has at least two trucks".into());
        }
        Ok(self)
    }

    pub fn home_lead(&self) -> bool {
        self.service_lines.iter().any(|s| s == "home_move")
    }

    pub fn works_on(&self, day: NaiveDate) -> bool {
        let weekday = i16::try_from(day.weekday().num_days_from_monday()).unwrap_or(0);
        self.working_days.contains(&weekday)
    }
}

/// How many home-move teams one date can take, before bookings are counted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DayCapacity {
    pub date: NaiveDate,
    /// Home-move leads working that day.
    pub leads: i64,
    /// Their jobs a day, summed.
    pub lead_jobs: i64,
    pub multi_truck_leads: i64,
    pub multi_truck_jobs: i64,
    pub international_leads: i64,
}

/// Per date from `from` to `to` inclusive: the home-move leads working it
/// (not off), and the jobs they can take.
pub fn home_capacity(leads: &[(ProviderProfile, Vec<NaiveDate>)], from: NaiveDate, to: NaiveDate) -> Vec<DayCapacity> {
    let mut out = Vec::new();
    let mut day = from;
    while day <= to && out.len() < 62 {
        let working: Vec<&ProviderProfile> = leads
            .iter()
            .filter(|(p, off)| p.home_lead() && p.works_on(day) && !off.contains(&day))
            .map(|(p, _)| p)
            .collect();
        out.push(DayCapacity {
            date: day,
            leads: working.len() as i64,
            lead_jobs: working.iter().map(|p| i64::from(p.max_daily_jobs)).sum(),
            multi_truck_leads: working.iter().filter(|p| p.multi_truck_capable).count() as i64,
            multi_truck_jobs: working.iter().filter(|p| p.multi_truck_capable).map(|p| i64::from(p.max_daily_jobs)).sum(),
            international_leads: working.iter().filter(|p| p.coverage.iter().any(|c| c == "international")).count() as i64,
        });
        match day.succ_opt() {
            Some(next) => day = next,
            None => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lead() -> ProviderProfile {
        ProviderProfile { service_lines: vec!["home_move".into()], registered_helpers: 4, ..ProviderProfile::default() }
    }

    #[test]
    fn a_profile_is_normalised_and_held_to_its_bounds() {
        let p = ProviderProfile {
            service_lines: vec!["Home_Move".into(), "freight_move".into(), "home_move".into()],
            working_days: vec![5, 0, 5],
            ..ProviderProfile::default()
        }
        .validated()
        .unwrap();
        assert_eq!(p.service_lines, ["freight_move", "home_move"]);
        assert_eq!(p.working_days, [0, 5]);

        assert!(ProviderProfile { service_lines: vec![], ..lead() }.validated().is_err());
        assert!(ProviderProfile { coverage: vec!["mars".into()], ..lead() }.validated().is_err());
        assert!(ProviderProfile { working_days: vec![7], ..lead() }.validated().is_err());
        assert!(ProviderProfile { multi_truck_capable: true, fleet_trucks: 1, ..lead() }.validated().is_err());
        assert!(ProviderProfile { max_daily_jobs: 0, ..lead() }.validated().is_err());
    }

    #[test]
    fn everyone_onboarded_before_this_stays_a_freight_driver() {
        let d = ProviderProfile::default();
        assert!(!d.home_lead());
        assert_eq!(d.coverage, ["local"]);
    }

    #[test]
    fn capacity_counts_home_leads_working_and_not_off() {
        // Mon 21 and Sun 27 Sep 2026.
        let mon = NaiveDate::from_ymd_opt(2026, 9, 21).unwrap();
        let sun = NaiveDate::from_ymd_opt(2026, 9, 27).unwrap();
        let enterprise = ProviderProfile { multi_truck_capable: true, fleet_trucks: 2, max_daily_jobs: 2, ..lead() };
        let intl = ProviderProfile { coverage: vec!["international".into(), "local".into()], ..lead() };
        let leads = vec![
            (lead(), vec![mon]),                  // off on Monday
            (enterprise, vec![]),
            (intl, vec![]),
            (ProviderProfile::default(), vec![]), // freight only: never counted
        ];
        let days = home_capacity(&leads, mon, sun);
        assert_eq!(days.len(), 7);
        let monday = &days[0];
        assert_eq!((monday.leads, monday.lead_jobs), (2, 3));
        assert_eq!((monday.multi_truck_leads, monday.multi_truck_jobs, monday.international_leads), (1, 2, 1));
        let tuesday = &days[1];
        assert_eq!(tuesday.leads, 3, "the lead off on Monday works Tuesday");
        assert_eq!(days[6].leads, 0, "nobody works Sunday by default");
    }
}

//! Provider profiles and availability. A driver with no row is a freight
//! driver on the defaults — nothing onboarded before this changes.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::NaiveDate;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::value_objects::provider::ProviderProfile;

#[async_trait]
pub trait ProviderStore: Send + Sync {
    /// The profile, the defaults when none is stored; None when the driver is
    /// not in this tenant.
    async fn get(&self, tenant_id: Uuid, driver_id: Uuid) -> anyhow::Result<Option<ProviderProfile>>;
    /// False when the driver is not in this tenant.
    async fn put(&self, tenant_id: Uuid, driver_id: Uuid, p: &ProviderProfile) -> anyhow::Result<bool>;
    async fn off_days(&self, tenant_id: Uuid, driver_id: Uuid, from: NaiveDate) -> anyhow::Result<Vec<NaiveDate>>;
    /// The lead's own working days and off days, replaced whole. False when
    /// the driver is not in this tenant.
    async fn set_availability(&self, tenant_id: Uuid, driver_id: Uuid, working_days: &[i16], off_days: &[NaiveDate]) -> anyhow::Result<bool>;
    /// Every active home-move lead in the tenant, with their off days from `from`.
    async fn home_leads(&self, tenant_id: Uuid, from: NaiveDate) -> anyhow::Result<Vec<(ProviderProfile, Vec<NaiveDate>)>>;
}

pub struct PgProviderStore {
    pool: PgPool,
}

impl PgProviderStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn to_profile(r: &sqlx::postgres::PgRow) -> ProviderProfile {
    ProviderProfile {
        service_lines: r.get("service_lines"),
        coverage: r.get("coverage"),
        multi_truck_capable: r.get("multi_truck_capable"),
        fleet_trucks: r.get("fleet_trucks"),
        registered_helpers: r.get("registered_helpers"),
        max_daily_jobs: r.get("max_daily_jobs"),
        working_days: r.get("working_days"),
    }
}

#[async_trait]
impl ProviderStore for PgProviderStore {
    async fn get(&self, tenant_id: Uuid, driver_id: Uuid) -> anyhow::Result<Option<ProviderProfile>> {
        let exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM driver_ops.drivers WHERE id = $1 AND tenant_id = $2")
            .bind(driver_id)
            .bind(tenant_id)
            .fetch_optional(&self.pool)
            .await?;
        if exists.is_none() {
            return Ok(None);
        }
        let row = sqlx::query(
            "SELECT service_lines, coverage, multi_truck_capable, fleet_trucks, registered_helpers,
                    max_daily_jobs, working_days
               FROM driver_ops.provider_profiles WHERE driver_id = $1 AND tenant_id = $2",
        )
        .bind(driver_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(Some(row.as_ref().map(to_profile).unwrap_or_default()))
    }

    async fn put(&self, tenant_id: Uuid, driver_id: Uuid, p: &ProviderProfile) -> anyhow::Result<bool> {
        let done = sqlx::query(
            "INSERT INTO driver_ops.provider_profiles
                    (driver_id, tenant_id, service_lines, coverage, multi_truck_capable, fleet_trucks,
                     registered_helpers, max_daily_jobs, working_days)
             SELECT d.id, d.tenant_id, $3, $4, $5, $6, $7, $8, $9
               FROM driver_ops.drivers d WHERE d.id = $1 AND d.tenant_id = $2
             ON CONFLICT (driver_id) DO UPDATE SET
                service_lines = EXCLUDED.service_lines, coverage = EXCLUDED.coverage,
                multi_truck_capable = EXCLUDED.multi_truck_capable, fleet_trucks = EXCLUDED.fleet_trucks,
                registered_helpers = EXCLUDED.registered_helpers, max_daily_jobs = EXCLUDED.max_daily_jobs,
                working_days = EXCLUDED.working_days, updated_at = NOW()",
        )
        .bind(driver_id)
        .bind(tenant_id)
        .bind(&p.service_lines)
        .bind(&p.coverage)
        .bind(p.multi_truck_capable)
        .bind(p.fleet_trucks)
        .bind(p.registered_helpers)
        .bind(p.max_daily_jobs)
        .bind(&p.working_days)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() == 1)
    }

    async fn off_days(&self, tenant_id: Uuid, driver_id: Uuid, from: NaiveDate) -> anyhow::Result<Vec<NaiveDate>> {
        let rows: Vec<(NaiveDate,)> = sqlx::query_as(
            "SELECT day FROM driver_ops.provider_off_days WHERE driver_id = $1 AND tenant_id = $2 AND day >= $3 ORDER BY day",
        )
        .bind(driver_id)
        .bind(tenant_id)
        .bind(from)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(d,)| d).collect())
    }

    async fn set_availability(&self, tenant_id: Uuid, driver_id: Uuid, working_days: &[i16], off_days: &[NaiveDate]) -> anyhow::Result<bool> {
        let mut tx = self.pool.begin().await?;
        // Working days live on the profile; a lead with no profile yet gets
        // the defaults with their own days.
        let defaults = ProviderProfile::default();
        let done = sqlx::query(
            "INSERT INTO driver_ops.provider_profiles
                    (driver_id, tenant_id, service_lines, coverage, working_days)
             SELECT d.id, d.tenant_id, $3, $4, $5
               FROM driver_ops.drivers d WHERE d.id = $1 AND d.tenant_id = $2
             ON CONFLICT (driver_id) DO UPDATE SET working_days = EXCLUDED.working_days, updated_at = NOW()",
        )
        .bind(driver_id)
        .bind(tenant_id)
        .bind(&defaults.service_lines)
        .bind(&defaults.coverage)
        .bind(working_days)
        .execute(&mut *tx)
        .await?;
        if done.rows_affected() != 1 {
            return Ok(false);
        }
        sqlx::query("DELETE FROM driver_ops.provider_off_days WHERE driver_id = $1 AND tenant_id = $2")
            .bind(driver_id)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO driver_ops.provider_off_days (driver_id, tenant_id, day)
             SELECT $1, $2, UNNEST($3::date[]) ON CONFLICT DO NOTHING",
        )
        .bind(driver_id)
        .bind(tenant_id)
        .bind(off_days)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    async fn home_leads(&self, tenant_id: Uuid, from: NaiveDate) -> anyhow::Result<Vec<(ProviderProfile, Vec<NaiveDate>)>> {
        let rows = sqlx::query(
            "SELECT p.driver_id, p.service_lines, p.coverage, p.multi_truck_capable, p.fleet_trucks,
                    p.registered_helpers, p.max_daily_jobs, p.working_days,
                    COALESCE(ARRAY(SELECT o.day FROM driver_ops.provider_off_days o
                                    WHERE o.driver_id = p.driver_id AND o.day >= $2), '{}') AS off_days
               FROM driver_ops.provider_profiles p
               JOIN driver_ops.drivers d ON d.id = p.driver_id AND d.is_active
              WHERE p.tenant_id = $1 AND 'home_move' = ANY(p.service_lines)",
        )
        .bind(tenant_id)
        .bind(from)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| (to_profile(r), r.get::<Vec<NaiveDate>, _>("off_days"))).collect())
    }
}

/// For tests and for a deployment without the table yet: everyone is a
/// freight driver on the defaults.
#[derive(Default)]
pub struct InMemoryProviderStore {
    profiles: Mutex<HashMap<(Uuid, Uuid), ProviderProfile>>,
    off: Mutex<HashMap<(Uuid, Uuid), Vec<NaiveDate>>>,
    /// Drivers that exist, per tenant. Empty means every driver exists.
    pub known: Mutex<Vec<(Uuid, Uuid)>>,
}

impl InMemoryProviderStore {
    fn exists(&self, tenant_id: Uuid, driver_id: Uuid) -> bool {
        let known = self.known.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        known.is_empty() || known.contains(&(tenant_id, driver_id))
    }
}

#[async_trait]
impl ProviderStore for InMemoryProviderStore {
    async fn get(&self, tenant_id: Uuid, driver_id: Uuid) -> anyhow::Result<Option<ProviderProfile>> {
        if !self.exists(tenant_id, driver_id) {
            return Ok(None);
        }
        let profiles = self.profiles.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(Some(profiles.get(&(tenant_id, driver_id)).cloned().unwrap_or_default()))
    }
    async fn put(&self, tenant_id: Uuid, driver_id: Uuid, p: &ProviderProfile) -> anyhow::Result<bool> {
        if !self.exists(tenant_id, driver_id) {
            return Ok(false);
        }
        self.profiles.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert((tenant_id, driver_id), p.clone());
        Ok(true)
    }
    async fn off_days(&self, tenant_id: Uuid, driver_id: Uuid, from: NaiveDate) -> anyhow::Result<Vec<NaiveDate>> {
        let off = self.off.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(off.get(&(tenant_id, driver_id)).map(|d| d.iter().copied().filter(|x| *x >= from).collect()).unwrap_or_default())
    }
    async fn set_availability(&self, tenant_id: Uuid, driver_id: Uuid, working_days: &[i16], off_days: &[NaiveDate]) -> anyhow::Result<bool> {
        if !self.exists(tenant_id, driver_id) {
            return Ok(false);
        }
        let mut profiles = self.profiles.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        profiles.entry((tenant_id, driver_id)).or_default().working_days = working_days.to_vec();
        self.off.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert((tenant_id, driver_id), off_days.to_vec());
        Ok(true)
    }
    async fn home_leads(&self, tenant_id: Uuid, from: NaiveDate) -> anyhow::Result<Vec<(ProviderProfile, Vec<NaiveDate>)>> {
        let profiles = self.profiles.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let off = self.off.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(profiles
            .iter()
            .filter(|((t, _), p)| *t == tenant_id && p.home_lead())
            .map(|(k, p)| (p.clone(), off.get(k).map(|d| d.iter().copied().filter(|x| *x >= from).collect()).unwrap_or_default()))
            .collect())
    }
}

//! Partner memberships against a real database.
//!
//! A membership decides which partner a token is bound to, so what these prove
//! is the rule that matters most: a user acts for at most one partner at a time.
//!
//! Needs Postgres with identity's migrations applied. Skipped locally when
//! DATABASE_URL is unset; fatal in CI, where a missing database means the
//! harness broke and a green pass over nothing would be worse than no test.

use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

use logisticos_identity::domain::entities::NewPartnerMembership;
use logisticos_identity::domain::repositories::{PartnerBinding, PartnerMembershipRepository, UpsertOutcome};
use logisticos_identity::infrastructure::db::PgPartnerMembershipRepository;

fn database_url() -> Option<String> {
    match std::env::var("DATABASE_URL") {
        Ok(url) => Some(url),
        Err(_) => {
            assert!(
                std::env::var("CI").is_err(),
                "DATABASE_URL is unset while CI is set: Postgres provisioning failed. \
                 These tests guard which partner a token is bound to; they must not be skipped here."
            );
            eprintln!("skipping: DATABASE_URL not set (this is fatal when CI is set)");
            None
        }
    }
}

async fn repo() -> Option<(PgPartnerMembershipRepository, PgPool)> {
    let url = database_url()?;
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.expect("connect");
    Some((PgPartnerMembershipRepository::new(pool.clone()), pool))
}

fn new_membership(tenant: Uuid, user: Uuid, carrier: Uuid) -> NewPartnerMembership {
    NewPartnerMembership { tenant_id: tenant, user_id: user, carrier_id: carrier, role: "store_manager".into() }
}

fn active(outcome: UpsertOutcome) -> logisticos_identity::domain::entities::PartnerMembership {
    match outcome {
        UpsertOutcome::Active(m) => m,
        other => panic!("expected an active membership, got {other:?}"),
    }
}

#[tokio::test]
async fn active_membership_is_found() {
    let Some((repo, _)) = repo().await else { return };
    let (t, u, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    repo.upsert_active(&new_membership(t, u, c)).await.unwrap();
    let found = repo.find_active(t, u).await.unwrap().expect("the membership");
    assert_eq!(found.carrier_id, c);
    assert_eq!(found.status, "active");
}

#[tokio::test]
async fn another_tenants_user_has_no_membership() {
    let Some((repo, _)) = repo().await else { return };
    let (t, u, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    repo.upsert_active(&new_membership(t, u, c)).await.unwrap();
    assert!(repo.find_active(Uuid::new_v4(), u).await.unwrap().is_none());
}

#[tokio::test]
async fn suspended_membership_is_not_found() {
    let Some((repo, pool)) = repo().await else { return };
    let (t, u, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    repo.upsert_active(&new_membership(t, u, c)).await.unwrap();
    sqlx::query("UPDATE identity.partner_memberships SET status = 'suspended' WHERE tenant_id = $1 AND user_id = $2")
        .bind(t).bind(u).execute(&pool).await.unwrap();
    assert!(repo.find_active(t, u).await.unwrap().is_none());
}

#[tokio::test]
async fn second_active_membership_for_a_user_is_refused() {
    let Some((repo, _)) = repo().await else { return };
    let (t, u) = (Uuid::new_v4(), Uuid::new_v4());
    let (first, second) = (Uuid::new_v4(), Uuid::new_v4());
    repo.upsert_active(&new_membership(t, u, first)).await.unwrap();

    match repo.upsert_active(&new_membership(t, u, second)).await.unwrap() {
        UpsertOutcome::ConflictsWith(existing) => assert_eq!(existing, first),
        other => panic!("expected a conflict, got {other:?}"),
    }
    // The token must still resolve to the first partner, not guess between two.
    assert_eq!(repo.find_active(t, u).await.unwrap().unwrap().carrier_id, first);
}

#[tokio::test]
async fn upsert_is_idempotent() {
    let Some((repo, pool)) = repo().await else { return };
    let (t, u, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let a = active(repo.upsert_active(&new_membership(t, u, c)).await.unwrap());
    let b = active(repo.upsert_active(&new_membership(t, u, c)).await.unwrap());
    assert_eq!(a.id, b.id);
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM identity.partner_memberships WHERE tenant_id = $1 AND user_id = $2")
        .bind(t).bind(u).fetch_one(&pool).await.unwrap();
    assert_eq!(rows, 1);
}

/// An approval retried after an admin suspended the user must not undo the
/// suspension. The suspended row comes back as it is.
#[tokio::test]
async fn upsert_does_not_reactivate_a_suspended_membership() {
    let Some((repo, pool)) = repo().await else { return };
    let (t, u, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    repo.upsert_active(&new_membership(t, u, c)).await.unwrap();
    sqlx::query("UPDATE identity.partner_memberships SET status = 'suspended' WHERE tenant_id = $1 AND user_id = $2")
        .bind(t).bind(u).execute(&pool).await.unwrap();

    match repo.upsert_active(&new_membership(t, u, c)).await.unwrap() {
        UpsertOutcome::Suspended(m) => assert_eq!(m.status, "suspended"),
        other => panic!("expected the suspended row back, got {other:?}"),
    }
    assert!(repo.find_active(t, u).await.unwrap().is_none());
}

#[tokio::test]
async fn concurrent_upserts_for_two_carriers_leave_exactly_one_active() {
    let Some((repo, pool)) = repo().await else { return };
    let repo = std::sync::Arc::new(repo);
    let (t, u) = (Uuid::new_v4(), Uuid::new_v4());
    let (c1, c2) = (Uuid::new_v4(), Uuid::new_v4());

    let (r1, r2) = (repo.clone(), repo.clone());
    let (a, b) = tokio::join!(
        async move { r1.upsert_active(&new_membership(t, u, c1)).await.unwrap() },
        async move { r2.upsert_active(&new_membership(t, u, c2)).await.unwrap() },
    );
    let winners = [&a, &b].iter().filter(|o| matches!(o, UpsertOutcome::Active(_))).count();
    assert_eq!(winners, 1, "exactly one carrier wins: {a:?} / {b:?}");

    let active_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM identity.partner_memberships WHERE tenant_id = $1 AND user_id = $2 AND status = 'active'")
        .bind(t).bind(u).fetch_one(&pool).await.unwrap();
    assert_eq!(active_rows, 1);
}

#[tokio::test]
async fn no_membership_is_unbound() {
    let Some((repo, _)) = repo().await else { return };
    let b = repo.binding(Uuid::new_v4(), Uuid::new_v4()).await.unwrap();
    assert_eq!(b, PartnerBinding::Unbound);
}

#[tokio::test]
async fn active_membership_is_bound() {
    let Some((repo, _)) = repo().await else { return };
    let (t, u, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    repo.upsert_active(&new_membership(t, u, c)).await.unwrap();
    assert_eq!(repo.binding(t, u).await.unwrap(), PartnerBinding::Active(c));
}

/// The case that must not look like "no partner": suspending a rider must not
/// hand them the tenant-wide driver token.
#[tokio::test]
async fn suspended_only_membership_is_reported_as_such() {
    let Some((repo, pool)) = repo().await else { return };
    let (t, u, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    repo.upsert_active(&new_membership(t, u, c)).await.unwrap();
    sqlx::query("UPDATE identity.partner_memberships SET status = 'suspended' WHERE tenant_id = $1 AND user_id = $2")
        .bind(t).bind(u).execute(&pool).await.unwrap();
    assert_eq!(repo.binding(t, u).await.unwrap(), PartnerBinding::SuspendedOnly);
}

/// A user suspended at one partner but active at another is still bound.
#[tokio::test]
async fn active_membership_wins_over_a_suspended_one() {
    let Some((repo, pool)) = repo().await else { return };
    let (t, u) = (Uuid::new_v4(), Uuid::new_v4());
    let (old, current) = (Uuid::new_v4(), Uuid::new_v4());
    repo.upsert_active(&new_membership(t, u, old)).await.unwrap();
    sqlx::query("UPDATE identity.partner_memberships SET status = 'suspended' WHERE tenant_id = $1 AND user_id = $2")
        .bind(t).bind(u).execute(&pool).await.unwrap();
    repo.upsert_active(&new_membership(t, u, current)).await.unwrap();
    assert_eq!(repo.binding(t, u).await.unwrap(), PartnerBinding::Active(current));
}

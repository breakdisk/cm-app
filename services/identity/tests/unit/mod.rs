use chrono::Utc;
use logisticos_auth::{
    claims::Claims,
    jwt::JwtService,
    password::{hash_password, verify_password},
    rbac::{default_permissions_for_role, permissions},
};
use logisticos_types::{ApiKeyId, SubscriptionTier, TenantId};
use uuid::Uuid;

use logisticos_identity::domain::entities::{ApiKey, Tenant, User};

// ---------------------------------------------------------------------------
// Tenant domain
// ---------------------------------------------------------------------------

mod tenant_tests {
    use super::*;

    #[test]
    fn new_tenant_is_active_by_default() {
        let t = Tenant::new(
            "FastShip PH".into(),
            "fastship-ph".into(),
            "owner@fastship.ph".into(),
        );
        assert!(t.is_active);
    }

    #[test]
    fn new_tenant_defaults_to_starter_tier() {
        let t = Tenant::new("Acme".into(), "acme".into(), "owner@acme.com".into());
        assert_eq!(t.subscription_tier, SubscriptionTier::Starter);
    }

    #[test]
    fn tenant_stores_name_and_slug() {
        let t = Tenant::new(
            "FastShip PH".into(),
            "fastship-ph".into(),
            "owner@fastship.ph".into(),
        );
        assert_eq!(t.name, "FastShip PH");
        assert_eq!(t.slug, "fastship-ph");
    }

    #[test]
    fn validate_slug_accepts_valid_slug() {
        assert!(Tenant::validate_slug("fastship-ph").is_ok());
        assert!(Tenant::validate_slug("acme123").is_ok());
        assert!(Tenant::validate_slug("abc").is_ok());
    }

    #[test]
    fn validate_slug_rejects_uppercase() {
        assert!(Tenant::validate_slug("FastShip").is_err());
    }

    #[test]
    fn validate_slug_rejects_spaces() {
        assert!(Tenant::validate_slug("fast ship").is_err());
    }

    #[test]
    fn validate_slug_rejects_leading_hyphen() {
        assert!(Tenant::validate_slug("-fastship").is_err());
    }

    #[test]
    fn validate_slug_rejects_trailing_hyphen() {
        assert!(Tenant::validate_slug("fastship-").is_err());
    }

    #[test]
    fn validate_slug_rejects_too_short() {
        assert!(Tenant::validate_slug("ab").is_err());
    }

    #[test]
    fn validate_slug_rejects_too_long() {
        let long = "a".repeat(51);
        assert!(Tenant::validate_slug(&long).is_err());
    }

    #[test]
    fn validate_slug_rejects_special_chars() {
        assert!(Tenant::validate_slug("foo&bar").is_err());
        assert!(Tenant::validate_slug("foo_bar").is_err());
    }

    #[test]
    fn starter_tenant_cannot_reactivate_after_deactivation() {
        let mut t = Tenant::new("Acme".into(), "acme".into(), "owner@acme.com".into());
        assert_eq!(t.subscription_tier, SubscriptionTier::Starter);
        t.deactivate();
        assert!(!t.is_active);
        assert!(!t.can_reactivate()); // Starter tier blocks re-activation
    }

    #[test]
    fn non_starter_tenant_can_reactivate_after_deactivation() {
        let mut t = Tenant::new("Acme".into(), "acme".into(), "owner@acme.com".into());
        t.upgrade_tier(SubscriptionTier::Growth);
        t.deactivate();
        assert!(!t.is_active);
        assert!(t.can_reactivate());
    }

    #[test]
    fn upgrade_tier_changes_subscription() {
        let mut t = Tenant::new("Acme".into(), "acme".into(), "owner@acme.com".into());
        t.upgrade_tier(SubscriptionTier::Enterprise);
        assert_eq!(t.subscription_tier, SubscriptionTier::Enterprise);
    }

    #[test]
    fn subscription_tier_starter_has_shipment_limit() {
        assert_eq!(SubscriptionTier::Starter.max_monthly_shipments(), Some(500));
    }

    #[test]
    fn subscription_tier_enterprise_has_no_shipment_limit() {
        assert_eq!(SubscriptionTier::Enterprise.max_monthly_shipments(), None);
    }

    #[test]
    fn starter_does_not_allow_ai_features() {
        assert!(!SubscriptionTier::Starter.allows_ai_features());
    }

    #[test]
    fn business_and_enterprise_allow_ai_features() {
        assert!(SubscriptionTier::Business.allows_ai_features());
        assert!(SubscriptionTier::Enterprise.allows_ai_features());
    }

    #[test]
    fn only_enterprise_allows_white_label() {
        assert!(!SubscriptionTier::Starter.allows_white_label());
        assert!(!SubscriptionTier::Business.allows_white_label());
        assert!(SubscriptionTier::Enterprise.allows_white_label());
    }
}

// ---------------------------------------------------------------------------
// User domain
// ---------------------------------------------------------------------------

mod user_tests {
    use super::*;

    fn make_tenant_id() -> TenantId {
        TenantId::new()
    }

    #[test]
    fn new_user_is_active_and_unverified() {
        let u = User::new(
            make_tenant_id(),
            "Alice@Example.com".into(),
            "hash".into(),
            "Alice".into(),
            "Smith".into(),
            vec!["merchant".into()],
        );
        assert!(u.is_active);
        assert!(!u.email_verified);
    }

    #[test]
    fn new_user_has_no_last_login() {
        let u = User::new(
            make_tenant_id(),
            "alice@example.com".into(),
            "hash".into(),
            "Alice".into(),
            "Smith".into(),
            vec![],
        );
        assert!(u.last_login_at.is_none());
    }

    #[test]
    fn can_login_requires_active_and_verified() {
        let mut u = User::new(
            make_tenant_id(),
            "alice@example.com".into(),
            "hash".into(),
            "Alice".into(),
            "Smith".into(),
            vec![],
        );
        assert!(!u.can_login()); // not verified yet

        u.email_verified = true;
        assert!(u.can_login()); // active + verified

        u.is_active = false;
        assert!(!u.can_login()); // deactivated
    }

    #[test]
    fn full_name_concatenates_first_and_last() {
        let u = User::new(
            make_tenant_id(),
            "alice@example.com".into(),
            "hash".into(),
            "Alice".into(),
            "Smith".into(),
            vec![],
        );
        assert_eq!(u.full_name(), "Alice Smith");
    }

    #[test]
    fn assign_role_adds_role_once() {
        let mut u = User::new(
            make_tenant_id(),
            "alice@example.com".into(),
            "hash".into(),
            "Alice".into(),
            "Smith".into(),
            vec!["merchant".into()],
        );
        u.assign_role("dispatcher");
        assert!(u.roles.contains(&"dispatcher".to_owned()));
        assert_eq!(u.roles.len(), 2);
    }

    #[test]
    fn assign_role_is_idempotent() {
        let mut u = User::new(
            make_tenant_id(),
            "alice@example.com".into(),
            "hash".into(),
            "Alice".into(),
            "Smith".into(),
            vec!["merchant".into()],
        );
        u.assign_role("merchant");
        assert_eq!(u.roles.len(), 1);
    }

    #[test]
    fn revoke_role_removes_role() {
        let mut u = User::new(
            make_tenant_id(),
            "alice@example.com".into(),
            "hash".into(),
            "Alice".into(),
            "Smith".into(),
            vec!["merchant".into(), "dispatcher".into()],
        );
        u.revoke_role("dispatcher");
        assert!(!u.roles.contains(&"dispatcher".to_owned()));
        assert_eq!(u.roles.len(), 1);
    }

    #[test]
    fn record_login_sets_last_login_at() {
        let mut u = User::new(
            make_tenant_id(),
            "alice@example.com".into(),
            "hash".into(),
            "Alice".into(),
            "Smith".into(),
            vec![],
        );
        assert!(u.last_login_at.is_none());
        u.record_login();
        assert!(u.last_login_at.is_some());
    }
}

// ---------------------------------------------------------------------------
// ApiKey domain
// ---------------------------------------------------------------------------

mod api_key_tests {
    use super::*;

    fn make_active_key() -> ApiKey {
        ApiKey {
            id: ApiKeyId::new(),
            tenant_id: TenantId::new(),
            name: "Test Key".into(),
            key_hash: "sha256hash".into(),
            key_prefix: "lsk_live_ab12".into(),
            scopes: vec!["shipments:read".into()],
            is_active: true,
            expires_at: None,
            last_used_at: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn active_non_expiring_key_is_valid() {
        let key = make_active_key();
        assert!(key.is_valid());
    }

    #[test]
    fn revoked_key_is_invalid() {
        let mut key = make_active_key();
        key.revoke();
        assert!(!key.is_valid());
    }

    #[test]
    fn expired_key_is_invalid() {
        let mut key = make_active_key();
        key.expires_at = Some(Utc::now() - chrono::Duration::seconds(1));
        assert!(!key.is_valid());
    }

    #[test]
    fn future_expiry_key_is_valid() {
        let mut key = make_active_key();
        key.expires_at = Some(Utc::now() + chrono::Duration::days(365));
        assert!(key.is_valid());
    }

    #[test]
    fn record_usage_sets_last_used_at() {
        let mut key = make_active_key();
        assert!(key.last_used_at.is_none());
        key.record_usage();
        assert!(key.last_used_at.is_some());
    }
}

// ---------------------------------------------------------------------------
// Password hashing (libs/auth)
// ---------------------------------------------------------------------------

mod password_tests {
    use super::*;

    #[test]
    fn hash_produces_non_empty_string() {
        let hash = hash_password("s3cr3tPass!").expect("hashing failed");
        assert!(!hash.is_empty());
    }

    #[test]
    fn hash_is_not_equal_to_plaintext() {
        let plaintext = "s3cr3tPass!";
        let hash = hash_password(plaintext).expect("hashing failed");
        assert_ne!(hash, plaintext);
    }

    #[test]
    fn two_hashes_of_same_password_differ_due_to_salt() {
        let h1 = hash_password("samepassword").expect("hashing failed");
        let h2 = hash_password("samepassword").expect("hashing failed");
        assert_ne!(h1, h2, "Argon2id should produce different hashes due to random salt");
    }

    #[test]
    fn verify_correct_password_succeeds() {
        let plaintext = "correctH0rs3Battery!";
        let hash = hash_password(plaintext).expect("hashing failed");
        assert!(verify_password(plaintext, &hash).is_ok());
    }

    #[test]
    fn verify_wrong_password_fails() {
        let hash = hash_password("rightPassword").expect("hashing failed");
        assert!(verify_password("wrongPassword", &hash).is_err());
    }

    #[test]
    fn verify_empty_password_against_non_empty_hash_fails() {
        let hash = hash_password("nonempty").expect("hashing failed");
        assert!(verify_password("", &hash).is_err());
    }

    #[test]
    fn verify_malformed_hash_string_fails() {
        assert!(verify_password("anything", "not-a-real-hash").is_err());
    }
}

// ---------------------------------------------------------------------------
// JWT (libs/auth)
// ---------------------------------------------------------------------------

mod jwt_tests {
    use super::*;

    fn make_service() -> JwtService {
        JwtService::new("test-secret-key-logisticos", 3600, 86400)
    }

    fn make_claims(expiry_seconds: i64) -> Claims {
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        Claims::new(
            user_id,
            tenant_id,
            "acme-logistics".into(),
            "business".into(),
            "user@acme.com".into(),
            vec!["admin".into()],
            vec![permissions::SHIPMENT_CREATE.into(), permissions::DISPATCH_ASSIGN.into()],
            expiry_seconds,
        )
    }

    #[test]
    fn issue_access_token_returns_non_empty_string() {
        let svc = make_service();
        let claims = make_claims(3600);
        let token = svc.issue_access_token(claims).expect("token creation failed");
        assert!(!token.is_empty());
    }

    #[test]
    fn validate_access_token_round_trips_claims() {
        let svc = make_service();
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let claims = Claims::new(
            user_id,
            tenant_id,
            "test-slug".into(),
            "enterprise".into(),
            "test@test.com".into(),
            vec!["merchant".into()],
            vec![permissions::SHIPMENT_READ.into()],
            3600,
        );

        let token = svc.issue_access_token(claims).expect("token creation failed");
        let data = svc.validate_access_token(&token).expect("validation failed");

        assert_eq!(data.claims.user_id, user_id);
        assert_eq!(data.claims.tenant_id, tenant_id);
        assert_eq!(data.claims.email, "test@test.com");
        assert_eq!(data.claims.tenant_slug, "test-slug");
        assert_eq!(data.claims.subscription_tier, "enterprise");
    }

    #[test]
    fn validate_access_token_fails_on_tampered_token() {
        let svc = make_service();
        let claims = make_claims(3600);
        let mut token = svc.issue_access_token(claims).expect("token creation failed");

        // Corrupt the signature portion (last segment of the JWT).
        let dot_pos = token.rfind('.').unwrap();
        token.push_str("tampered");
        let _ = token; // suppress unused warning — we already modified it in-place via push_str

        // Re-derive: manipulate the token to corrupt signature
        let svc2 = make_service();
        let original = svc2.issue_access_token(make_claims(3600)).unwrap();
        let parts: Vec<&str> = original.splitn(3, '.').collect();
        assert_eq!(parts.len(), 3);
        let tampered = format!("{}.{}.invalidsignatureXXXXXXXXXXXX", parts[0], parts[1]);

        let result = svc.validate_access_token(&tampered);
        assert!(result.is_err(), "Tampered token should fail validation");
    }

    #[test]
    fn validate_access_token_fails_on_expired_token() {
        let svc = make_service();
        let claims = make_claims(-1); // expiry = 1 second in the past
        let token = svc.issue_access_token(claims).expect("token creation failed");
        let result = svc.validate_access_token(&token);
        assert!(result.is_err(), "Expired token should fail validation");
        // Should specifically be a TokenExpired error.
        let err = result.unwrap_err();
        assert!(
            matches!(err, logisticos_auth::error::AuthError::TokenExpired),
            "Expected TokenExpired, got: {:?}",
            err
        );
    }

    #[test]
    fn validate_access_token_fails_with_wrong_secret() {
        let svc_a = JwtService::new("secret-a", 3600, 86400);
        let svc_b = JwtService::new("secret-b", 3600, 86400);
        let claims = make_claims(3600);
        let token = svc_a.issue_access_token(claims).expect("token creation failed");
        let result = svc_b.validate_access_token(&token);
        assert!(result.is_err(), "Token signed with different secret should fail");
    }

    #[test]
    fn claims_contain_correct_tenant_and_user_ids() {
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let claims = Claims::new(
            user_id,
            tenant_id,
            "slug".into(),
            "starter".into(),
            "u@t.com".into(),
            vec![],
            vec![],
            3600,
        );
        assert_eq!(claims.user_id, user_id);
        assert_eq!(claims.tenant_id, tenant_id);
        assert_eq!(claims.sub, user_id.to_string());
    }

    #[test]
    fn claims_exp_is_in_the_future() {
        let claims = make_claims(3600);
        let now = Utc::now().timestamp();
        assert!(claims.exp > now, "exp should be in the future");
    }

    #[test]
    fn claims_jti_is_unique_per_token() {
        let c1 = make_claims(3600);
        let c2 = make_claims(3600);
        assert_ne!(c1.jti, c2.jti, "Each token must have a unique jti");
    }
}

// ---------------------------------------------------------------------------
// RBAC permissions (libs/auth)
// ---------------------------------------------------------------------------

mod rbac_tests {
    use super::*;

    fn claims_for_role(role: &str) -> Claims {
        let perms: Vec<String> = default_permissions_for_role(role)
            .into_iter()
            .map(|s| s.to_owned())
            .collect();
        Claims::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "test-slug".into(),
            "business".into(),
            "test@test.com".into(),
            vec![role.to_owned()],
            perms,
            3600,
        )
    }

    // Admin role
    #[test]
    fn admin_has_shipment_create() {
        let c = claims_for_role("admin");
        assert!(c.has_permission(permissions::SHIPMENT_CREATE));
    }

    #[test]
    fn admin_has_dispatch_assign() {
        let c = claims_for_role("admin");
        assert!(c.has_permission(permissions::DISPATCH_ASSIGN));
    }

    #[test]
    fn admin_has_users_manage() {
        let c = claims_for_role("admin");
        assert!(c.has_permission(permissions::USERS_MANAGE));
    }

    #[test]
    fn admin_has_payments_reconcile() {
        let c = claims_for_role("admin");
        assert!(c.has_permission(permissions::PAYMENTS_RECONCILE));
    }

    #[test]
    fn admin_has_analytics_export() {
        let c = claims_for_role("admin");
        assert!(c.has_permission(permissions::ANALYTICS_EXPORT));
    }

    // Driver role
    #[test]
    fn driver_has_shipment_read() {
        let c = claims_for_role("driver");
        assert!(c.has_permission(permissions::SHIPMENT_READ));
    }

    #[test]
    fn driver_has_dispatch_view() {
        let c = claims_for_role("driver");
        assert!(c.has_permission(permissions::DISPATCH_VIEW));
    }

    #[test]
    fn driver_cannot_create_shipments() {
        let c = claims_for_role("driver");
        assert!(!c.has_permission(permissions::SHIPMENT_CREATE));
    }

    #[test]
    fn driver_cannot_manage_users() {
        let c = claims_for_role("driver");
        assert!(!c.has_permission(permissions::USERS_MANAGE));
    }

    // Merchant role
    #[test]
    fn merchant_has_shipment_create() {
        let c = claims_for_role("merchant");
        assert!(c.has_permission(permissions::SHIPMENT_CREATE));
    }

    #[test]
    fn merchant_has_shipment_cancel() {
        let c = claims_for_role("merchant");
        assert!(c.has_permission(permissions::SHIPMENT_CANCEL));
    }

    #[test]
    fn merchant_cannot_assign_dispatch() {
        let c = claims_for_role("merchant");
        assert!(!c.has_permission(permissions::DISPATCH_ASSIGN));
    }

    #[test]
    fn merchant_cannot_manage_users() {
        let c = claims_for_role("merchant");
        assert!(!c.has_permission(permissions::USERS_MANAGE));
    }

    // Finance role
    #[test]
    fn finance_has_payments_read() {
        let c = claims_for_role("finance");
        assert!(c.has_permission(permissions::PAYMENTS_READ));
    }

    #[test]
    fn finance_has_payments_export() {
        let c = claims_for_role("finance");
        assert!(c.has_permission(permissions::PAYMENTS_EXPORT));
    }

    #[test]
    fn finance_cannot_create_shipments() {
        let c = claims_for_role("finance");
        assert!(!c.has_permission(permissions::SHIPMENT_CREATE));
    }

    // has_permission / has_role
    #[test]
    fn has_permission_returns_false_for_unknown_permission() {
        let c = claims_for_role("readonly");
        assert!(!c.has_permission("nonexistent:permission"));
    }

    #[test]
    fn wildcard_permission_grants_everything() {
        let c = Claims::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "slug".into(),
            "enterprise".into(),
            "sa@logisticos.io".into(),
            vec!["superadmin".into()],
            vec!["*".into()], // wildcard
            3600,
        );
        assert!(c.has_permission(permissions::SHIPMENT_CREATE));
        assert!(c.has_permission(permissions::DISPATCH_ASSIGN));
        assert!(c.has_permission("any:permission:at:all"));
    }

    #[test]
    fn has_role_returns_true_for_assigned_role() {
        let c = claims_for_role("dispatcher");
        assert!(c.has_role("dispatcher"));
    }

    #[test]
    fn has_role_returns_false_for_unassigned_role() {
        let c = claims_for_role("dispatcher");
        assert!(!c.has_role("admin"));
    }

    // can_use_ai
    #[test]
    fn starter_claims_cannot_use_ai() {
        let mut c = claims_for_role("admin");
        c.subscription_tier = "starter".into();
        assert!(!c.can_use_ai());
    }

    #[test]
    fn business_claims_can_use_ai() {
        let mut c = claims_for_role("admin");
        c.subscription_tier = "business".into();
        assert!(c.can_use_ai());
    }

    #[test]
    fn enterprise_claims_can_use_ai() {
        let mut c = claims_for_role("admin");
        c.subscription_tier = "enterprise".into();
        assert!(c.can_use_ai());
    }

    // Unknown role
    #[test]
    fn unknown_role_has_no_permissions() {
        let perms = default_permissions_for_role("undefined_role");
        assert!(perms.is_empty());
    }
}

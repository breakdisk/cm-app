use std::sync::Arc;
use std::fmt::Write;
use logisticos_auth::{jwt::JwtService, password::verify_password, claims::Claims, rbac::default_permissions_for_role};
use logisticos_errors::{AppError, AppResult};
use logisticos_events::{envelope::Event, topics, producer::KafkaProducer, payloads::OtpRequested};
use crate::{
    application::commands::{
        LoginCommand, LoginResult, RefreshTokenCommand, OtpSendCommand, OtpVerifyCommand, OtpVerifyResult,
        ExchangeFirebaseCommand, ExchangeFirebaseResult, ExchangedUser,
    },
    domain::{
        entities::{AuthIdentity, AuthProvider, Tenant},
        repositories::{TenantRepository, UserRepository, AuthIdentityRepository, PricingFeatureRepository},
    },
    infrastructure::db::user_repo::{PgPasswordResetTokenRepository, PgEmailVerificationTokenRepository},
    infrastructure::cache::RedisCache,
    infrastructure::external::EmailAdapter,
};

/// Permissions granted to a draft-tenant owner during lazy onboarding.
/// Intentionally narrow: they can only finalize the tenant and set up billing;
/// Domains for addresses minted from a phone number for OTP-only sign-in.
///
/// Not mailboxes: nothing can ever be delivered to `<digits>@customer…` or
/// `<digits>@driver…`, so "verify your email" is unsatisfiable for these
/// accounts. Their phone was the thing verified.
const SYNTHESIZED_EMAIL_DOMAINS: &[&str] =
    &["@customer.logisticos.app", "@driver.logisticos.app"];

/// Was this address minted from a phone number rather than supplied by a person?
fn is_synthesized_login_email(email: &str) -> bool {
    SYNTHESIZED_EMAIL_DOMAINS.iter().any(|d| email.ends_with(d))
}

/// Is the fixed development OTP (`123456`) accepted, and may generated codes be
/// written to the log?
///
/// **Fails closed.** This used to be `std::env::var("APP__ENV") != "production"`,
/// which is open by default twice over: `unwrap_or_default()` yields `""` when
/// the variable is unset, and `""` is not `"production"`. The internet-facing
/// VPS also runs with `APP__ENV=development`, so on 2026-08-14 anyone could
/// POST /v1/auth/otp/verify with `123456` for any phone number and receive a
/// working customer token — verified against the live host. Every OTP was also
/// being written to the container log in plaintext.
///
/// Now both require `AUTH__ALLOW_DEV_OTP` to be set explicitly, *and* the
/// environment not to be production. A missing or misspelt variable disables
/// the shortcut rather than enabling it.
fn dev_otp_enabled() -> bool {
    let explicitly_allowed = matches!(
        std::env::var("AUTH__ALLOW_DEV_OTP").as_deref(),
        Ok("true") | Ok("1")
    );
    let is_production = std::env::var("APP__ENV").as_deref() == Ok("production");
    explicitly_allowed && !is_production
}

/// every other API call returns 403 `onboarding_required` until
/// `POST /v1/tenants/me/finalize` promotes the tenant to `active`.
const ONBOARDING_PERMISSIONS: &[&str] = &[
    logisticos_auth::rbac::permissions::TENANT_UPDATE_SELF,
    logisticos_auth::rbac::permissions::BILLING_SETUP,
];

pub struct AuthService {
    tenant_repo: Arc<dyn TenantRepository>,
    user_repo: Arc<dyn UserRepository>,
    auth_identity_repo: Arc<dyn AuthIdentityRepository>,
    pricing_feature_repo: Arc<dyn PricingFeatureRepository>,
    jwt: Arc<JwtService>,
    reset_token_repo: Arc<PgPasswordResetTokenRepository>,
    email_verification_token_repo: Arc<PgEmailVerificationTokenRepository>,
    redis_cache: Arc<RedisCache>,
    email: Arc<dyn EmailAdapter>,
    app_base_url: String,
    kafka: Arc<KafkaProducer>,
}

impl AuthService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tenant_repo: Arc<dyn TenantRepository>,
        user_repo: Arc<dyn UserRepository>,
        auth_identity_repo: Arc<dyn AuthIdentityRepository>,
        pricing_feature_repo: Arc<dyn PricingFeatureRepository>,
        jwt: Arc<JwtService>,
        reset_token_repo: Arc<PgPasswordResetTokenRepository>,
        email_verification_token_repo: Arc<PgEmailVerificationTokenRepository>,
        redis_cache: Arc<RedisCache>,
        email: Arc<dyn EmailAdapter>,
        app_base_url: String,
        kafka: Arc<KafkaProducer>,
    ) -> Self {
        Self { tenant_repo, user_repo, auth_identity_repo, pricing_feature_repo, jwt, reset_token_repo, email_verification_token_repo, redis_cache, email, app_base_url, kafka }
    }

    /// Resolve feature keys enabled for the given tier from the pricing matrix.
    /// Errors are logged and silently swallowed — a missing/failed DB lookup
    /// should never break login; the JWT just won't carry `enabled_features`.
    async fn features_for_tier(&self, tier: &str) -> Vec<String> {
        match self.pricing_feature_repo.list_for_tier(tier).await {
            Ok(features) => features.into_iter().map(|f| f.feature_key).collect(),
            Err(e) => {
                tracing::warn!(tier = %tier, error = %e, "Failed to load pricing features for tier; JWT will omit enabled_features");
                Vec::new()
            }
        }
    }

    pub async fn login(&self, cmd: LoginCommand) -> AppResult<LoginResult> {
        let tenant = self.tenant_repo
            .find_by_slug(&cmd.tenant_slug).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Tenant", id: cmd.tenant_slug.clone() })?;

        if !tenant.is_active {
            return Err(AppError::BusinessRule("Tenant account is suspended".into()));
        }

        let mut user = self.user_repo
            .find_by_email(&tenant.id, &cmd.email).await
            .map_err(AppError::Internal)?
            .ok_or(AppError::Unauthorized("Invalid credentials".into()))?;

        if !user.can_login() {
            return Err(AppError::Unauthorized("Account inactive or email not verified".into()));
        }

        verify_password(&cmd.password, &user.password_hash)
            .map_err(|_| AppError::Unauthorized("Invalid credentials".into()))?;

        let permissions: Vec<String> = user.roles.iter()
            .flat_map(|r| default_permissions_for_role(r))
            .map(|p| p.to_owned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let tier_str = format!("{:?}", tenant.subscription_tier).to_lowercase();
        let enabled_features = self.features_for_tier(&tier_str).await;
        let claims = Claims::new(
            user.id.inner(), tenant.id.inner(),
            tenant.slug.clone(),
            tier_str,
            user.email.clone(), user.roles.clone(), permissions,
            self.jwt.access_expiry_seconds(),
        ).with_features(enabled_features);
        let refresh_claims = logisticos_auth::claims::RefreshClaims::new(
            user.id.inner(), tenant.id.inner(), self.jwt.refresh_expiry_seconds(),
        );

        let access_token  = self.jwt.issue_access_token(claims).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        let refresh_token = self.jwt.issue_refresh_token(refresh_claims).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

        user.record_login();
        self.user_repo.save(&user).await.map_err(AppError::Internal)?;
        tracing::info!(user_id = %user.id, tenant_id = %tenant.id, "User logged in");

        Ok(LoginResult { access_token, refresh_token, expires_in: self.jwt.access_expiry_seconds(), token_type: "Bearer".into() })
    }

    pub async fn refresh(&self, cmd: RefreshTokenCommand) -> AppResult<LoginResult> {
        let data = self.jwt.validate_refresh_token(&cmd.refresh_token)
            .map_err(|e| AppError::Unauthorized(e.to_string()))?;

        let tenant_id = logisticos_types::TenantId::from_uuid(data.claims.tenant_id);
        let user_id   = logisticos_types::UserId::from_uuid(
            data.claims.sub.parse().map_err(|_| AppError::Unauthorized("Malformed token".into()))?
        );

        let tenant = self.tenant_repo.find_by_id(&tenant_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Unauthorized("Tenant not found".into()))?;
        let user = self.user_repo.find_by_id(&user_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Unauthorized("User not found".into()))?;

        if !user.can_login() || !tenant.is_active {
            return Err(AppError::Unauthorized("Account inactive".into()));
        }

        // Draft tenants retain the narrow onboarding permission set across
        // refreshes — only a successful `finalize_self` call (which flips the
        // tenant to `active`) upgrades the user to their full role-based perms
        // on the next refresh.
        let onboarding_required = tenant.is_draft();
        let permissions: Vec<String> = if onboarding_required {
            ONBOARDING_PERMISSIONS.iter().map(|p| (*p).to_owned()).collect()
        } else {
            user.roles.iter()
                .flat_map(|r| default_permissions_for_role(r))
                .map(|p| p.to_owned())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect()
        };

        let tier_str = format!("{:?}", tenant.subscription_tier).to_lowercase();
        let enabled_features = if onboarding_required {
            Vec::new()
        } else {
            self.features_for_tier(&tier_str).await
        };
        let claims = Claims::new(user.id.inner(), tenant.id.inner(), tenant.slug.clone(),
            tier_str,
            user.email.clone(), user.roles.clone(), permissions, self.jwt.access_expiry_seconds())
            .with_onboarding(onboarding_required)
            .with_features(enabled_features);
        let refresh_claims = logisticos_auth::claims::RefreshClaims::new(user.id.inner(), tenant.id.inner(), self.jwt.refresh_expiry_seconds());

        Ok(LoginResult {
            access_token:  self.jwt.issue_access_token(claims).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?,
            refresh_token: self.jwt.issue_refresh_token(refresh_claims).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?,
            expires_in: self.jwt.access_expiry_seconds(),
            token_type: "Bearer".into(),
        })
    }

    pub async fn forgot_password(&self, cmd: crate::application::commands::ForgotPasswordCommand) -> AppResult<()> {
        use validator::Validate;
        cmd.validate().map_err(|e| AppError::Validation(e.to_string()))?;

        let tenant = match self.tenant_repo.find_by_slug(&cmd.tenant_slug).await
            .map_err(AppError::Internal)?
        {
            Some(t) => t,
            None => return Ok(()),  // Don't reveal tenant existence
        };

        let user = self.user_repo.find_by_email(&tenant.id, &cmd.email).await
            .map_err(AppError::Internal)?;

        if let Some(user) = user {
            let raw_token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
            let token_hash = sha2_hash(raw_token.as_bytes());

            self.reset_token_repo
                .create_reset_token(user.id.inner(), tenant.id.inner(), &token_hash)
                .await
                .map_err(AppError::Internal)?;

            let reset_link = format!("{}/reset-password?token={raw_token}", self.app_base_url);
            let html = format!(
                "<p>Hi {},</p><p>Click the link below to reset your password. It expires in 1 hour.</p><p><a href=\"{reset_link}\">{reset_link}</a></p><p>If you did not request this, ignore this email.</p>",
                user.first_name
            );
            if let Err(e) = self.email.send(&user.email, "Reset your LogisticOS password", &html).await {
                tracing::warn!(error = %e, user_id = %user.id, "Failed to send password reset email");
            }
        }
        Ok(()) // Always return Ok to avoid email enumeration
    }

    pub async fn reset_password(&self, cmd: crate::application::commands::ResetPasswordCommand) -> AppResult<()> {
        use validator::Validate;
        cmd.validate().map_err(|e| AppError::Validation(e.to_string()))?;

        let token_hash = sha2_hash(cmd.token.as_bytes());

        let (user_id, _tenant_id) = self.reset_token_repo
            .claim_token(&token_hash).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Unauthorized("Invalid or expired reset token".into()))?;

        let user_id_typed = logisticos_types::UserId::from_uuid(user_id);
        let mut user = self.user_repo.find_by_id(&user_id_typed).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "User", id: user_id.to_string() })?;

        let new_hash = logisticos_auth::password::hash_password(&cmd.new_password)
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        user.password_hash = new_hash;
        user.updated_at = chrono::Utc::now();
        self.user_repo.save(&user).await.map_err(AppError::Internal)?;

        tracing::info!(user_id = %user_id, "Password reset completed");
        Ok(())
    }

    pub async fn send_verification_email(&self, cmd: crate::application::commands::SendVerificationEmailCommand) -> AppResult<()> {
        use validator::Validate;
        cmd.validate().map_err(|e| AppError::Validation(e.to_string()))?;

        let tenant = match self.tenant_repo.find_by_slug(&cmd.tenant_slug).await
            .map_err(AppError::Internal)?
        {
            Some(t) => t,
            None => return Ok(()),  // Don't reveal tenant existence
        };

        let user = self.user_repo.find_by_email(&tenant.id, &cmd.email).await
            .map_err(AppError::Internal)?;

        if let Some(user) = user {
            if user.email_verified {
                return Ok(());
            }
            let raw_token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
            let token_hash = sha2_hash(raw_token.as_bytes());

            self.email_verification_token_repo
                .create(user.id.inner(), tenant.id.inner(), &token_hash)
                .await
                .map_err(AppError::Internal)?;

            let verify_link = format!("{}/verify-email?token={raw_token}", self.app_base_url);
            let html = format!(
                "<p>Hi {},</p><p>Click the link below to verify your email address.</p><p><a href=\"{verify_link}\">{verify_link}</a></p>",
                user.first_name
            );
            if let Err(e) = self.email.send(&user.email, "Verify your LogisticOS email", &html).await {
                tracing::warn!(error = %e, user_id = %user.id, "Failed to send verification email");
            }
        }
        Ok(())
    }

    pub async fn register(&self, cmd: crate::application::commands::RegisterCommand) -> AppResult<()> {
        use validator::Validate;
        cmd.validate().map_err(|e| AppError::Validation(e.to_string()))?;

        // The phone-derived namespace is not claimable with a password.
        //
        // `<digits>@customer.logisticos.app` / `@driver…` are minted by the OTP
        // path from a verified phone number. This endpoint is public, so
        // without this check someone could register one of those addresses
        // with a password of their choosing and take the identity that phone
        // number resolves to within the tenant. Nobody owns these addresses —
        // no mail is deliverable to them — so there is no legitimate reason to
        // register one.
        if is_synthesized_login_email(&cmd.email) {
            return Err(AppError::Validation(
                "That email domain is reserved for phone sign-in. Use the OTP flow instead.".into(),
            ));
        }

        let tenant = self.tenant_repo.find_by_slug(&cmd.tenant_slug).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Tenant", id: cmd.tenant_slug.clone() })?;

        // Check if email already registered for this tenant.
        let existing = self.user_repo.find_by_email(&tenant.id, &cmd.email).await
            .map_err(AppError::Internal)?;
        if existing.is_some() {
            return Err(AppError::Conflict("Email already registered".into()));
        }

        let password_hash = logisticos_auth::password::hash_password(&cmd.password)
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

        // Assign role based on email convention:
        // - *@customer.logisticos.app → customer
        // - everything else defaults to driver (invite flow assigns explicit roles)
        let role = if cmd.email.ends_with("@customer.logisticos.app") {
            "customer"
        } else {
            "driver"
        };
        let mut user = crate::domain::entities::User::new(
            tenant.id.clone(),
            cmd.email.clone(),
            password_hash,
            cmd.first_name,
            cmd.last_name,
            vec![role.to_owned()],
        );

        // Deliberately no auto-verification here. The OTP sign-in path creates
        // its own users and sets email_verified itself ("OTP-verified phone =
        // verified identity"); nothing phone-derived reaches this function.

        self.user_repo.save(&user).await.map_err(AppError::Internal)?;

        // Send verification email (skipped in dev for customer accounts).
        if !user.email_verified {
            self.send_verification_email(crate::application::commands::SendVerificationEmailCommand {
                tenant_slug: cmd.tenant_slug,
                email: cmd.email,
            }).await?;
        }

        tracing::info!(user_id = %user.id, tenant_id = %tenant.id, "User registered");
        Ok(())
    }

    pub async fn verify_email(&self, cmd: crate::application::commands::VerifyEmailCommand) -> AppResult<()> {
        let token_hash = sha2_hash(cmd.token.as_bytes());

        let (user_id, _tenant_id) = self.email_verification_token_repo
            .claim_token(&token_hash).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Unauthorized("Invalid or expired verification token".into()))?;

        let user_id_typed = logisticos_types::UserId::from_uuid(user_id);
        let mut user = self.user_repo.find_by_id(&user_id_typed).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "User", id: user_id.to_string() })?;

        user.email_verified = true;
        user.updated_at = chrono::Utc::now();
        self.user_repo.save(&user).await.map_err(AppError::Internal)?;

        tracing::info!(user_id = %user_id, "Email verified");
        Ok(())
    }

    // ─── OTP-based authentication (driver app + customer app) ────────────────

    pub async fn otp_send(&self, cmd: OtpSendCommand) -> AppResult<()> {
        use validator::Validate;
        cmd.validate().map_err(|e| AppError::Validation(e.to_string()))?;

        // Require at least one of phone_number or email
        let identifier = cmd.identifier()
            .ok_or_else(|| AppError::Validation("phone_number or email is required".into()))?;

        // Generate a 6-digit OTP
        use rand::Rng;
        let otp: String = format!("{:06}", rand::thread_rng().gen_range(0..1_000_000u32));

        self.redis_cache.store_otp(identifier, &otp).await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;

        if cmd.email.is_some() {
            // Email-based OTP — publish to engagement engine for delivery.
            let event = Event::new(
                "identity",
                topics::OTP_REQUESTED,
                uuid::Uuid::nil(), // tenant unknown before verify; engagement ignores this for OTP
                OtpRequested { email: identifier.to_owned(), otp_code: otp.clone(), phone_number: String::new() },
            );
            match self.kafka.publish_event(topics::OTP_REQUESTED, &event).await {
                Ok(_) => tracing::info!(identifier = %identifier, "OTP_REQUESTED published to engagement engine"),
                Err(e) => {
                    tracing::warn!(identifier = %identifier, error = %e,
                        "Failed to publish OTP_REQUESTED — falling back to direct email");
                    let html = format!(
                        "<p>Your one-time verification code is: <strong>{otp}</strong></p>\
                         <p>This code expires in 5 minutes. Do not share it with anyone.</p>",
                    );
                    if let Err(mail_err) = self.email.send(identifier, "Your CargoMarket verification code", &html).await {
                        tracing::error!(identifier = %identifier, error = %mail_err,
                            "Kafka and direct email both failed — OTP not delivered");
                    }
                }
            }
        } else {
            // Phone-based OTP — publish to engagement engine for SMS delivery.
            let event = Event::new(
                "identity",
                topics::OTP_REQUESTED,
                uuid::Uuid::nil(),
                OtpRequested { email: String::new(), otp_code: otp.clone(), phone_number: identifier.to_owned() },
            );
            match self.kafka.publish_event(topics::OTP_REQUESTED, &event).await {
                Ok(_) => tracing::info!(identifier = %identifier, "OTP_REQUESTED (phone) published to engagement engine"),
                Err(e) => {
                    tracing::error!(identifier = %identifier, error = %e,
                        "Failed to publish phone OTP_REQUESTED — SMS not delivered");
                }
            }
        }

        // The code itself is a credential. Logged only when the development
        // shortcut is explicitly enabled — never merely because APP__ENV is
        // something other than "production".
        if dev_otp_enabled() {
            tracing::info!(identifier = %identifier, otp = %otp, "OTP generated (123456 also accepted)");
        }

        Ok(())
    }


    pub async fn otp_verify(&self, cmd: OtpVerifyCommand) -> AppResult<OtpVerifyResult> {
        use validator::Validate;
        cmd.validate().map_err(|e| AppError::Validation(e.to_string()))?;

        let identifier = cmd.identifier()
            .ok_or_else(|| AppError::Validation("phone_number or email is required".into()))?;

        let tenant_slug = cmd.tenant_slug.as_deref().unwrap_or("demo");

        // Opt-in only — see dev_otp_enabled().
        let otp_valid = if dev_otp_enabled() && cmd.otp_code == "123456" {
            true
        } else {
            self.redis_cache.verify_otp(identifier, &cmd.otp_code).await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?
        };

        if !otp_valid {
            return Err(AppError::Unauthorized("Invalid or expired OTP".into()));
        }

        // Resolve tenant
        let tenant = self.tenant_repo
            .find_by_slug(tenant_slug).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Tenant", id: tenant_slug.to_owned() })?;

        let role = cmd.role.as_deref().unwrap_or("driver");

        // ── Email-based OTP: look up directly by email ───────────────────────
        // Short-circuit the phone-based lookup path for email logins (customer app,
        // ops portal). The OTP was keyed on the email address in otp_send.
        let user = if let Some(ref email_addr) = cmd.email {
            self.user_repo
                .find_by_email(&tenant.id, email_addr).await
                .map_err(AppError::Internal)?
                .ok_or_else(|| AppError::NotFound { resource: "User", id: email_addr.clone() })?
        } else {
            // ── Phone-based OTP ───────────────────────────────────────────────
            // Normalise the phone number to E.164 before any lookup.
            let normalised_phone = crate::application::services::tenant_service::normalise_phone(
                cmd.phone_number.as_deref().unwrap_or_default()
            );

            // ── Step 1: try to find a pre-registered user by phone ───────────────
            // Partner portal admin registers drivers with a real email + phone.
            // The phone is stored on identity.users so the Driver App can log in
            // without the driver ever knowing their email address.
            if let Some(pre_registered) = self.user_repo
                .find_by_phone(&tenant.id, &normalised_phone)
                .await
                .map_err(AppError::Internal)?
            {
                tracing::info!(
                    user_id = %pre_registered.id,
                    phone = %normalised_phone,
                    "OTP login: resolved pre-registered user by phone"
                );
                pre_registered
            } else {
                // ── Step 2: fall back to synthetic-email find-or-create ──────────
                // Handles self-registering customers and dev/test drivers that were
                // never pre-registered through the Partner portal.
                //
                // For `role=driver`, hitting this branch is almost always a bug:
                // it means a partner-onboarded driver couldn't be resolved by phone
                // (most often because their `users.phone_number` is NULL or stored
                // in a non-E.164 format). The fallback then creates a *shadow*
                // identity user, and driver-ops `find_or_create_driver` keeps
                // creating "Driver" stub rows — the real onboarded driver row
                // stays offline forever. Surface it loudly in production logs so
                // ops can spot the data drift instead of staring at a healthy app
                // that's silently invisible to dispatch.
                if role == "driver" {
                    tracing::warn!(
                        phone = %normalised_phone,
                        tenant_id = %tenant.id,
                        "OTP login: no pre-registered driver found by phone — falling through to synthetic-email auto-create. This produces a shadow user; the partner-onboarded driver row will not be touched. Backfill identity.users.phone_number for this driver."
                    );
                }
                let digits: String = normalised_phone.chars().filter(|c| c.is_ascii_digit()).collect();
                let (email, password, first_name) = match role {
                    "customer" => (
                        format!("{digits}@customer.logisticos.app"),
                        format!("Cust{digits}!Lgx"),
                        "Customer".to_owned(),
                    ),
                    _ => (
                        format!("{digits}@driver.logisticos.app"),
                        format!("Drv{digits}!Lgx"),
                        "Driver".to_owned(),
                    ),
                };

                match self.user_repo.find_by_email(&tenant.id, &email).await.map_err(AppError::Internal)? {
                    Some(u) => u,
                    None => {
                        let password_hash = logisticos_auth::password::hash_password(&password)
                            .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
                        let mut new_user = crate::domain::entities::User::new(
                            tenant.id.clone(),
                            email.clone(),
                            password_hash,
                            first_name,
                            digits.clone(),
                            vec![role.to_owned()],
                        );
                        new_user.email_verified = true; // OTP-verified phone = verified identity
                        new_user.phone_number = Some(normalised_phone.clone());
                        self.user_repo.save(&new_user).await.map_err(AppError::Internal)?;
                        tracing::info!(user_id = %new_user.id, phone = %normalised_phone, role = %role, "Auto-registered user via OTP");
                        new_user
                    }
                }
            }
        };

        // Issue tokens
        let permissions: Vec<String> = user.roles.iter()
            .flat_map(|r| default_permissions_for_role(r))
            .map(|p| p.to_owned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let tier_str = format!("{:?}", tenant.subscription_tier).to_lowercase();
        let enabled_features = self.features_for_tier(&tier_str).await;
        let claims = Claims::new(
            user.id.inner(), tenant.id.inner(),
            tenant.slug.clone(),
            tier_str,
            user.email.clone(), user.roles.clone(), permissions,
            self.jwt.access_expiry_seconds(),
        ).with_features(enabled_features);
        let refresh_claims = logisticos_auth::claims::RefreshClaims::new(
            user.id.inner(), tenant.id.inner(), self.jwt.refresh_expiry_seconds(),
        );

        let access_token = self.jwt.issue_access_token(claims)
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        let refresh_token = self.jwt.issue_refresh_token(refresh_claims)
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

        tracing::info!(user_id = %user.id, tenant_id = %tenant.id, identifier = %identifier, "OTP login successful");

        Ok(OtpVerifyResult {
            access_token,
            refresh_token,
            driver_id: user.id.inner().to_string(),
            tenant_id: tenant.id.inner().to_string(),
            expires_in: self.jwt.access_expiry_seconds(),
            token_type: "Bearer".into(),
        })
    }

    // ─── Firebase → LogisticOS JWT exchange ──────────────────────────────────
    //
    // Called by the landing app (server-side) after it has verified the
    // Firebase ID token. Mints a LogisticOS access + refresh JWT bound to the
    // user's tenant, provisioning a draft tenant on first merchant sign-in and
    // auto-linking customers via signed white-label partner context.

    pub async fn exchange_firebase(&self, cmd: ExchangeFirebaseCommand) -> AppResult<ExchangeFirebaseResult> {
        use validator::Validate;
        cmd.validate().map_err(|e| AppError::Validation(e.to_string()))?;

        if !cmd.email_verified {
            return Err(AppError::Unauthorized("Firebase email not verified".into()));
        }

        // 1. Existing identity → mint directly for the linked user.
        if let Some(identity) = self
            .auth_identity_repo
            .find_by_provider_subject(AuthProvider::Firebase, &cmd.firebase_uid)
            .await
            .map_err(AppError::Internal)?
        {
            return self.mint_for_existing_user(identity.user_id).await;
        }

        // 2. No identity yet → lazy onboarding by role.
        match cmd.role.as_str() {
            "merchant" => self.provision_draft_merchant(&cmd).await,
            "customer" => self.provision_partner_customer(&cmd).await,
            // Invited admins and partners already have an identity row created by
            // `invite_user`. On first Firebase sign-in they have no auth_identity
            // link yet. Find their row by email (cross-tenant), validate the role
            // matches, create the link, then mint tokens normally.
            "admin" | "partner" | "tenant_admin" => self.link_invited_user(&cmd).await,
            other => Err(AppError::Validation(format!("unknown role: {other}"))),
        }
    }

    async fn link_invited_user(
        &self,
        cmd: &ExchangeFirebaseCommand,
    ) -> AppResult<ExchangeFirebaseResult> {
        let user = self
            .user_repo
            .find_by_email_global(&cmd.email)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Forbidden { resource: "user_not_invited".into() })?;

        if !user.is_active {
            return Err(AppError::Unauthorized("Account inactive".into()));
        }

        // Ensure the user actually holds the role the portal is requesting.
        let requested_role = &cmd.role;
        let allowed = match requested_role.as_str() {
            "admin" => user.roles.iter().any(|r| r == "admin" || r == "tenant_admin"),
            other   => user.roles.iter().any(|r| r == other),
        };
        if !allowed {
            return Err(AppError::Forbidden { resource: "role_mismatch".into() });
        }

        let tenant = self
            .tenant_repo
            .find_by_id(&user.tenant_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("invited user points to missing tenant")))?;

        let identity = AuthIdentity::new(
            user.id.clone(),
            AuthProvider::Firebase,
            cmd.firebase_uid.clone(),
            cmd.email.clone(),
        );
        self.auth_identity_repo.insert(&identity).await.map_err(AppError::Internal)?;

        tracing::info!(
            user_id = %user.id,
            tenant_id = %tenant.id,
            firebase_uid = %cmd.firebase_uid,
            role = %requested_role,
            "Linked Firebase UID to invited user"
        );

        let onboarding_required = tenant.is_draft();
        let permissions: Vec<String> = if onboarding_required {
            ONBOARDING_PERMISSIONS.iter().map(|p| (*p).to_owned()).collect()
        } else {
            user.roles.iter()
                .flat_map(|r| default_permissions_for_role(r))
                .map(|p| p.to_owned())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect()
        };

        self.build_exchange_result(&tenant, &user, permissions, onboarding_required).await
    }

    async fn mint_for_existing_user(
        &self,
        user_id: logisticos_types::UserId,
    ) -> AppResult<ExchangeFirebaseResult> {
        let user = self
            .user_repo
            .find_by_id(&user_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("auth_identity points to missing user {user_id}")))?;

        if !user.is_active {
            return Err(AppError::Unauthorized("Account inactive".into()));
        }

        let tenant = self
            .tenant_repo
            .find_by_id(&user.tenant_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("user {user_id} points to missing tenant")))?;

        if !tenant.is_active {
            return Err(AppError::BusinessRule("Tenant account is suspended".into()));
        }

        let onboarding_required = tenant.is_draft();
        let permissions = if onboarding_required {
            ONBOARDING_PERMISSIONS.iter().map(|p| (*p).to_owned()).collect()
        } else {
            user.roles.iter()
                .flat_map(|r| default_permissions_for_role(r))
                .map(|p| p.to_owned())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect()
        };

        self.build_exchange_result(&tenant, &user, permissions, onboarding_required).await
    }

    async fn provision_draft_merchant(
        &self,
        cmd: &ExchangeFirebaseCommand,
    ) -> AppResult<ExchangeFirebaseResult> {
        // Slug: draft-<first 8 chars of firebase uid>. Firebase UIDs are 28
        // chars of alphanumerics, already RFC-safe for a slug.
        let uid_prefix: String = cmd
            .firebase_uid
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(8)
            .collect::<String>()
            .to_ascii_lowercase();
        if uid_prefix.len() < 4 {
            return Err(AppError::Validation("firebase_uid too short for draft slug".into()));
        }
        let slug = format!("draft-{uid_prefix}");

        let tenant = Tenant::new_draft(slug.clone(), cmd.email.clone());
        self.tenant_repo.save(&tenant).await.map_err(AppError::Internal)?;

        let (first_name, last_name) = split_display_name(cmd.display_name.as_deref(), &cmd.email);
        let mut user = crate::domain::entities::User::new(
            tenant.id.clone(),
            cmd.email.clone(),
            String::new(), // no password — Firebase is the sole credential
            first_name,
            last_name,
            vec!["merchant".to_owned()],
        );
        user.email_verified = true; // Firebase already verified the email
        self.user_repo.save(&user).await.map_err(AppError::Internal)?;

        let identity = AuthIdentity::new(
            user.id.clone(),
            AuthProvider::Firebase,
            cmd.firebase_uid.clone(),
            cmd.email.clone(),
        );
        self.auth_identity_repo.insert(&identity).await.map_err(AppError::Internal)?;

        tracing::info!(
            tenant_id = %tenant.id,
            user_id = %user.id,
            firebase_uid = %cmd.firebase_uid,
            "Provisioned draft merchant tenant via Firebase exchange"
        );

        let permissions = ONBOARDING_PERMISSIONS.iter().map(|p| (*p).to_owned()).collect();
        self.build_exchange_result(&tenant, &user, permissions, true).await
    }

    async fn provision_partner_customer(
        &self,
        cmd: &ExchangeFirebaseCommand,
    ) -> AppResult<ExchangeFirebaseResult> {
        let partner_slug = cmd.partner_slug.as_deref().ok_or_else(|| AppError::Forbidden {
            resource: "tenant_required".into(),
        })?;
        let partner_sig = cmd.partner_sig.as_deref().ok_or_else(|| AppError::Forbidden {
            resource: "tenant_required".into(),
        })?;

        verify_partner_signature(partner_slug, &cmd.firebase_uid, partner_sig)?;

        let tenant = self
            .tenant_repo
            .find_by_slug(partner_slug)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Forbidden { resource: "tenant_required".into() })?;

        if !tenant.is_active || tenant.is_draft() {
            return Err(AppError::Forbidden { resource: "tenant_required".into() });
        }

        // Find-or-create user in the partner tenant.
        let user = match self
            .user_repo
            .find_by_email(&tenant.id, &cmd.email)
            .await
            .map_err(AppError::Internal)?
        {
            Some(u) => u,
            None => {
                let (first_name, last_name) = split_display_name(cmd.display_name.as_deref(), &cmd.email);
                let mut new_user = crate::domain::entities::User::new(
                    tenant.id.clone(),
                    cmd.email.clone(),
                    String::new(),
                    first_name,
                    last_name,
                    vec!["customer".to_owned()],
                );
                new_user.email_verified = true;
                self.user_repo.save(&new_user).await.map_err(AppError::Internal)?;
                new_user
            }
        };

        let identity = AuthIdentity::new(
            user.id.clone(),
            AuthProvider::Firebase,
            cmd.firebase_uid.clone(),
            cmd.email.clone(),
        );
        self.auth_identity_repo.insert(&identity).await.map_err(AppError::Internal)?;

        tracing::info!(
            tenant_id = %tenant.id,
            user_id = %user.id,
            partner_slug = %partner_slug,
            "Linked Firebase customer to partner tenant"
        );

        let permissions: Vec<String> = user.roles.iter()
            .flat_map(|r| default_permissions_for_role(r))
            .map(|p| p.to_owned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        self.build_exchange_result(&tenant, &user, permissions, false).await
    }

    async fn build_exchange_result(
        &self,
        tenant: &Tenant,
        user: &crate::domain::entities::User,
        permissions: Vec<String>,
        onboarding_required: bool,
    ) -> AppResult<ExchangeFirebaseResult> {
        let tier_str = format!("{:?}", tenant.subscription_tier).to_lowercase();
        let enabled_features = if onboarding_required {
            Vec::new()
        } else {
            self.features_for_tier(&tier_str).await
        };
        let claims = Claims::new(
            user.id.inner(),
            tenant.id.inner(),
            tenant.slug.clone(),
            tier_str,
            user.email.clone(),
            user.roles.clone(),
            permissions,
            self.jwt.access_expiry_seconds(),
        )
        .with_onboarding(onboarding_required)
        .with_features(enabled_features);
        let refresh_claims = logisticos_auth::claims::RefreshClaims::new(
            user.id.inner(),
            tenant.id.inner(),
            self.jwt.refresh_expiry_seconds(),
        );

        let access_token = self.jwt.issue_access_token(claims)
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        let refresh_token = self.jwt.issue_refresh_token(refresh_claims)
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

        Ok(ExchangeFirebaseResult {
            access_token,
            refresh_token,
            expires_in: self.jwt.access_expiry_seconds(),
            token_type: "Bearer".into(),
            user: ExchangedUser {
                id:                  user.id.inner().to_string(),
                tenant_id:            tenant.id.inner().to_string(),
                tenant_slug:          tenant.slug.clone(),
                email:                user.email.clone(),
                roles:                user.roles.clone(),
                onboarding_required,
            },
        })
    }
}

/// Verify the HMAC-SHA256 signature a white-label partner includes when
/// deep-linking a customer into their tenant:
///
/// Fenced as `text`. A four-space indent is Markdown's *other* code-block
/// syntax, and rustdoc compiles those as Rust too — so this pseudocode was
/// failing identity's doc-test job just as surely as an untagged fence would.
///
/// ```text
/// mac = HMAC-SHA256(LOGISTICOS_PARTNER_HMAC_SECRET, "<partner_slug>:<firebase_uid>")
/// sig = base64url(mac)
/// ```
fn verify_partner_signature(partner_slug: &str, firebase_uid: &str, sig_b64: &str) -> AppResult<()> {
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let secret = std::env::var("LOGISTICOS_PARTNER_HMAC_SECRET")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("LOGISTICOS_PARTNER_HMAC_SECRET not set")))?;

    let provided = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(sig_b64.as_bytes())
        .map_err(|_| AppError::Forbidden { resource: "tenant_required".into() })?;

    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes())
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
    mac.update(partner_slug.as_bytes());
    mac.update(b":");
    mac.update(firebase_uid.as_bytes());

    mac.verify_slice(&provided)
        .map_err(|_| AppError::Forbidden { resource: "tenant_required".into() })?;
    Ok(())
}

fn split_display_name(display_name: Option<&str>, email: &str) -> (String, String) {
    if let Some(name) = display_name.filter(|s| !s.trim().is_empty()) {
        let mut parts = name.trim().splitn(2, ' ');
        let first = parts.next().unwrap_or("").to_owned();
        let last = parts.next().unwrap_or("").to_owned();
        return (first, last);
    }
    let local = email.split('@').next().unwrap_or(email);
    (local.to_owned(), String::new())
}

fn sha2_hash(data: &[u8]) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    result.iter().fold(String::new(), |mut s, b| { write!(s, "{b:02x}").ok(); s })
}

#[cfg(test)]
mod dev_otp_gate_tests {
    use super::dev_otp_enabled;

    /// These mutate process-wide env, so they run in one test to keep the
    /// ordering deterministic — separate #[test] fns race each other.
    #[test]
    fn the_development_otp_shortcut_fails_closed() {
        // SAFETY: single-threaded within this test; no other test reads these.
        unsafe {
            std::env::remove_var("AUTH__ALLOW_DEV_OTP");
            std::env::remove_var("APP__ENV");
        }
        assert!(!dev_otp_enabled(), "unset must mean OFF — this was the bug: an \
             absent APP__ENV read as non-production and enabled the bypass");

        unsafe { std::env::set_var("APP__ENV", "development") }
        assert!(
            !dev_otp_enabled(),
            "a development environment alone must NOT enable it — the live VPS \
             runs APP__ENV=development and was accepting 123456 from the internet"
        );

        unsafe { std::env::set_var("AUTH__ALLOW_DEV_OTP", "true") }
        assert!(dev_otp_enabled(), "explicit opt-in in a dev environment enables it");

        // Production wins over the opt-in, so shipping the flag by accident is
        // still not enough to open it.
        unsafe { std::env::set_var("APP__ENV", "production") }
        assert!(!dev_otp_enabled(), "production must refuse even when opted in");

        unsafe {
            std::env::set_var("APP__ENV", "development");
            std::env::set_var("AUTH__ALLOW_DEV_OTP", "yes");
        }
        assert!(!dev_otp_enabled(), "only 'true'/'1' count — a typo means OFF");

        unsafe {
            std::env::remove_var("AUTH__ALLOW_DEV_OTP");
            std::env::remove_var("APP__ENV");
        }
    }
}

#[cfg(test)]
mod synthesized_email_tests {
    use super::is_synthesized_login_email;

    /// These belong to the OTP path, which mints them from a verified phone
    /// number. `register()` refuses them so the namespace cannot be claimed
    /// with a password.
    #[test]
    fn phone_derived_addresses_are_recognised() {
        assert!(is_synthesized_login_email("639170000123@customer.logisticos.app"));
        assert!(is_synthesized_login_email("971553604321@driver.logisticos.app"));
    }

    /// A real address registers normally.
    #[test]
    fn real_addresses_are_not() {
        assert!(!is_synthesized_login_email("eduard@example.com"));
        assert!(!is_synthesized_login_email("admin@demo.com"));
        // Near-misses: the suffix has to be the actual domain, not a prefix of
        // some other host that merely starts the same way.
        assert!(!is_synthesized_login_email("x@customer.logisticos.app.evil.com"));
        assert!(!is_synthesized_login_email("x@notcustomer.logisticos.app.co"));
    }
}

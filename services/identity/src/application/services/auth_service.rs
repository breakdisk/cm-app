use std::sync::Arc;
use std::fmt::Write;
use logisticos_auth::{jwt::JwtService, password::verify_password, claims::Claims, rbac::default_permissions_for_role};
use logisticos_errors::{AppError, AppResult};
use crate::{
    application::commands::{
        LoginCommand, LoginResult, RefreshTokenCommand, OtpSendCommand, OtpVerifyCommand, OtpVerifyResult,
        ExchangeFirebaseCommand, ExchangeFirebaseResult, ExchangedUser,
    },
    domain::{
        entities::{AuthIdentity, AuthProvider, Tenant},
        repositories::{TenantRepository, UserRepository, AuthIdentityRepository},
    },
    infrastructure::db::user_repo::{PgPasswordResetTokenRepository, PgEmailVerificationTokenRepository},
    infrastructure::cache::RedisCache,
};

/// Permissions granted to a draft-tenant owner during lazy onboarding.
/// Intentionally narrow: they can only finalize the tenant and set up billing;
/// every other API call returns 403 `onboarding_required` until
/// `POST /v1/tenants/me/finalize` promotes the tenant to `active`.
const ONBOARDING_PERMISSIONS: &[&str] = &[
    "tenants:update-self",
    "billing:setup",
];

pub struct AuthService {
    tenant_repo: Arc<dyn TenantRepository>,
    user_repo: Arc<dyn UserRepository>,
    auth_identity_repo: Arc<dyn AuthIdentityRepository>,
    jwt: Arc<JwtService>,
    reset_token_repo: Arc<PgPasswordResetTokenRepository>,
    email_verification_token_repo: Arc<PgEmailVerificationTokenRepository>,
    redis_cache: Arc<RedisCache>,
}

impl AuthService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tenant_repo: Arc<dyn TenantRepository>,
        user_repo: Arc<dyn UserRepository>,
        auth_identity_repo: Arc<dyn AuthIdentityRepository>,
        jwt: Arc<JwtService>,
        reset_token_repo: Arc<PgPasswordResetTokenRepository>,
        email_verification_token_repo: Arc<PgEmailVerificationTokenRepository>,
        redis_cache: Arc<RedisCache>,
    ) -> Self {
        Self { tenant_repo, user_repo, auth_identity_repo, jwt, reset_token_repo, email_verification_token_repo, redis_cache }
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

        let claims = Claims::new(
            user.id.inner(), tenant.id.inner(),
            tenant.slug.clone(),
            format!("{:?}", tenant.subscription_tier).to_lowercase(),
            user.email.clone(), user.roles.clone(), permissions,
            self.jwt.access_expiry_seconds(),
        );
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

        let permissions: Vec<String> = user.roles.iter()
            .flat_map(|r| default_permissions_for_role(r))
            .map(|p| p.to_owned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let claims = Claims::new(user.id.inner(), tenant.id.inner(), tenant.slug.clone(),
            format!("{:?}", tenant.subscription_tier).to_lowercase(),
            user.email.clone(), user.roles.clone(), permissions, self.jwt.access_expiry_seconds());
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

            tracing::info!(
                user_id = %user.id,
                reset_link = %format!("http://localhost:3002/reset-password?token={raw_token}"),
                "Password reset token generated — use this link in dev"
            );
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

            tracing::info!(
                user_id = %user.id,
                verify_link = %format!("http://localhost:3002/verify-email?token={raw_token}"),
                "Email verification token generated — use this link in dev"
            );
        }
        Ok(())
    }

    pub async fn register(&self, cmd: crate::application::commands::RegisterCommand) -> AppResult<()> {
        use validator::Validate;
        cmd.validate().map_err(|e| AppError::Validation(e.to_string()))?;

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

        // In development, auto-verify customer accounts so the mobile app flow works.
        let env = std::env::var("APP__ENV").unwrap_or_default();
        if env == "development" && user.email.ends_with("@customer.logisticos.app") {
            user.email_verified = true;
        }

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

        // Generate a 6-digit OTP
        use rand::Rng;
        let otp: String = format!("{:06}", rand::thread_rng().gen_range(0..1_000_000u32));

        self.redis_cache.store_otp(&cmd.phone_number, &otp).await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;

        // In production this would dispatch to the engagement engine via Kafka
        // to send the OTP via SMS/WhatsApp. For now, log it in dev.
        let env = std::env::var("APP__ENV").unwrap_or_default();
        if env == "development" {
            tracing::info!(phone = %cmd.phone_number, otp = %otp, "DEV OTP generated (also accept 123456)");
        }

        Ok(())
    }

    pub async fn otp_verify(&self, cmd: OtpVerifyCommand) -> AppResult<OtpVerifyResult> {
        use validator::Validate;
        cmd.validate().map_err(|e| AppError::Validation(e.to_string()))?;

        let tenant_slug = cmd.tenant_slug.as_deref().unwrap_or("demo");
        let env = std::env::var("APP__ENV").unwrap_or_default();

        // Dev bypass: always accept 123456
        let otp_valid = if env == "development" && cmd.otp_code == "123456" {
            true
        } else {
            self.redis_cache.verify_otp(&cmd.phone_number, &cmd.otp_code).await
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

        // Derive a synthetic email from the phone number (digits only)
        let digits: String = cmd.phone_number.chars().filter(|c| c.is_ascii_digit()).collect();
        let role = cmd.role.as_deref().unwrap_or("driver");
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

        // Find-or-create user
        let user = match self.user_repo.find_by_email(&tenant.id, &email).await.map_err(AppError::Internal)? {
            Some(u) => u,
            None => {
                // Auto-register
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
                self.user_repo.save(&new_user).await.map_err(AppError::Internal)?;
                tracing::info!(user_id = %new_user.id, phone = %cmd.phone_number, role = %role, "Auto-registered user via OTP");
                new_user
            }
        };

        // Issue tokens
        let permissions: Vec<String> = user.roles.iter()
            .flat_map(|r| default_permissions_for_role(r))
            .map(|p| p.to_owned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let claims = Claims::new(
            user.id.inner(), tenant.id.inner(),
            tenant.slug.clone(),
            format!("{:?}", tenant.subscription_tier).to_lowercase(),
            user.email.clone(), user.roles.clone(), permissions,
            self.jwt.access_expiry_seconds(),
        );
        let refresh_claims = logisticos_auth::claims::RefreshClaims::new(
            user.id.inner(), tenant.id.inner(), self.jwt.refresh_expiry_seconds(),
        );

        let access_token = self.jwt.issue_access_token(claims)
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        let refresh_token = self.jwt.issue_refresh_token(refresh_claims)
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

        tracing::info!(user_id = %user.id, tenant_id = %tenant.id, phone = %cmd.phone_number, "OTP login successful");

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
            "admin" | "partner" => Err(AppError::Forbidden {
                resource: "tenant_not_provisioned".into(),
            }),
            other => Err(AppError::Validation(format!("unknown role: {other}"))),
        }
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

        self.build_exchange_result(&tenant, &user, permissions, onboarding_required)
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
        self.build_exchange_result(&tenant, &user, permissions, true)
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

        self.build_exchange_result(&tenant, &user, permissions, false)
    }

    fn build_exchange_result(
        &self,
        tenant: &Tenant,
        user: &crate::domain::entities::User,
        permissions: Vec<String>,
        onboarding_required: bool,
    ) -> AppResult<ExchangeFirebaseResult> {
        let claims = Claims::new(
            user.id.inner(),
            tenant.id.inner(),
            tenant.slug.clone(),
            format!("{:?}", tenant.subscription_tier).to_lowercase(),
            user.email.clone(),
            user.roles.clone(),
            permissions,
            self.jwt.access_expiry_seconds(),
        );
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
///     mac = HMAC-SHA256(LOGISTICOS_PARTNER_HMAC_SECRET, "<partner_slug>:<firebase_uid>")
///     sig = base64url(mac)
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

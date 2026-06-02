use std::sync::Arc;
use logisticos_auth::password::hash_password;
use logisticos_errors::{AppError, AppResult};
use logisticos_events::{producer::KafkaProducer, topics, payloads::{TenantCreated, TenantFinalized, UserCreated}, envelope::Event};
use crate::{
    application::commands::{CreateTenantCommand, FinalizeTenantCommand, GenerateDriverInviteLinkResult, InviteUserCommand},
    domain::{
        entities::{Tenant, User},
        repositories::{TenantRepository, UserRepository},
    },
    infrastructure::db::PgDriverInviteTokenRepository,
};

pub struct TenantService {
    tenant_repo: Arc<dyn TenantRepository>,
    user_repo: Arc<dyn UserRepository>,
    kafka: Arc<KafkaProducer>,
    driver_invite_token_repo: Arc<PgDriverInviteTokenRepository>,
}

impl TenantService {
    pub fn new(
        tenant_repo: Arc<dyn TenantRepository>,
        user_repo: Arc<dyn UserRepository>,
        kafka: Arc<KafkaProducer>,
        driver_invite_token_repo: Arc<PgDriverInviteTokenRepository>,
    ) -> Self {
        Self { tenant_repo, user_repo, kafka, driver_invite_token_repo }
    }

    pub async fn create_tenant(&self, cmd: CreateTenantCommand) -> AppResult<Tenant> {
        // Validate slug format (domain rule enforced in entity)
        Tenant::validate_slug(&cmd.slug)
            .map_err(|e| AppError::Validation(e.to_string()))?;

        // Uniqueness check
        if self.tenant_repo.slug_exists(&cmd.slug).await.map_err(AppError::Internal)? {
            return Err(AppError::BusinessRule(format!("Slug '{}' is already taken", cmd.slug)));
        }

        let tenant = Tenant::new(cmd.name, cmd.slug, cmd.owner_email.clone());

        // Hash owner password — Argon2id, never store plaintext
        let password_hash = hash_password(&cmd.owner_password)
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

        // Create the owning user with tenant_admin role
        let owner = User::new(
            tenant.id.clone(),
            cmd.owner_email,
            password_hash,
            cmd.owner_first_name,
            cmd.owner_last_name,
            vec!["tenant_admin".to_string()],
        );

        // Persist both in sequence — tenant first (FK constraint)
        self.tenant_repo.save(&tenant).await.map_err(AppError::Internal)?;
        self.user_repo.save(&owner).await.map_err(AppError::Internal)?;

        // Publish event — other services react to this (engagement sends welcome email, etc.)
        // Fire-and-forget: a missing Kafka topic must not roll back tenant creation.
        let event = Event::new(
            "identity",
            "tenant.created",
            tenant.id.inner(),
            TenantCreated {
                tenant_id: tenant.id.inner(),
                name: tenant.name.clone(),
                slug: tenant.slug.clone(),
                owner_email: tenant.owner_email.clone(),
                owner_user_id: owner.id.inner(),
                subscription_tier: format!("{:?}", tenant.subscription_tier).to_lowercase(),
            },
        );
        if let Err(e) = self.kafka.publish_event(topics::TENANT_CREATED, &event).await {
            tracing::warn!(tenant_id = %tenant.id, error = %e,
                "Failed to publish TENANT_CREATED event — tenant saved, downstream consumers may miss this");
        }

        tracing::info!(tenant_id = %tenant.id, slug = %tenant.slug, "Tenant created");
        Ok(tenant)
    }

    /// Expose the user repo for read operations from HTTP handlers.
    pub fn user_repo_ref(&self) -> Arc<dyn crate::domain::repositories::UserRepository> {
        Arc::clone(&self.user_repo)
    }

    /// Expose the tenant repo for read-only paths (e.g. GET /v1/tenants/me).
    pub fn tenant_repo_ref(&self) -> Arc<dyn crate::domain::repositories::TenantRepository> {
        Arc::clone(&self.tenant_repo)
    }

    /// Promote the caller's own tenant from `draft` to `active`. Called via
    /// `POST /v1/tenants/me/finalize` from the lazy-onboarding `/setup` flow.
    /// Idempotent for already-active tenants — re-calling just updates the
    /// business name. The caller's next refresh_token call will receive the
    /// full role-based permission set in the refreshed access JWT.
    ///
    /// Currency and region arrive in the command but are only logged until
    /// a follow-up migration extends the `identity.tenants` schema with
    /// those columns; they are already validated (ISO 4217 / 3166-1 alpha-2).
    pub async fn finalize_self(
        &self,
        tenant_id: &logisticos_types::TenantId,
        cmd: FinalizeTenantCommand,
    ) -> AppResult<Tenant> {
        use validator::Validate;
        cmd.validate().map_err(|e| AppError::Validation(e.to_string()))?;

        let mut tenant = self.tenant_repo
            .find_by_id(tenant_id).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "Tenant",
                id: tenant_id.inner().to_string(),
            })?;

        let was_draft = tenant.is_draft();
        tenant.finalize(cmd.business_name.clone())
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        self.tenant_repo.save(&tenant).await.map_err(AppError::Internal)?;

        tracing::info!(
            tenant_id = %tenant.id,
            was_draft,
            currency = %cmd.currency,
            region = %cmd.region,
            "Tenant finalized"
        );

        // Emit tenant.finalized — consumed by engagement (welcome flow) and
        // billing (create billing account). Fire-and-forget; Kafka outages
        // must not block tenant activation.
        let fin_event = Event::new(
            "logisticos/identity",
            "tenant.finalized",
            tenant.id.inner(),
            TenantFinalized {
                tenant_id:    tenant.id.inner(),
                name:         tenant.name.clone(),
                owner_email:  tenant.owner_email.clone(),
                finalized_at: chrono::Utc::now().to_rfc3339(),
            },
        );
        if let Err(e) = self.kafka.publish_event(topics::TENANT_FINALIZED, &fin_event).await {
            tracing::warn!(
                error = %e,
                tenant_id = %tenant.id,
                "TenantFinalized event publish failed (non-fatal)"
            );
        }

        Ok(tenant)
    }

    /// Tenant self-edit — partial update of name + owner_email. Subscription
    /// tier and is_active are NOT exposed here; those go through dedicated
    /// /upgrade-tier and /suspend endpoints (with billing/ops side-effects).
    /// Slug is intentionally immutable: cross-service references (auth-bridge,
    /// driver-app TENANT_ID, marketplace partner_slug) all key off it.
    pub async fn update_tenant(
        &self,
        tenant_id: &logisticos_types::TenantId,
        cmd: crate::application::commands::UpdateTenantCommand,
    ) -> AppResult<Tenant> {
        use validator::Validate;
        cmd.validate().map_err(|e| AppError::Validation(e.to_string()))?;

        let mut tenant = self.tenant_repo
            .find_by_id(tenant_id).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "Tenant",
                id: tenant_id.inner().to_string(),
            })?;

        if let Some(name) = cmd.name {
            tenant.name = name;
        }
        if let Some(email) = cmd.owner_email {
            tenant.owner_email = email;
        }
        tenant.updated_at = chrono::Utc::now();
        self.tenant_repo.save(&tenant).await.map_err(AppError::Internal)?;

        tracing::info!(tenant_id = %tenant.id, "Tenant profile updated");
        Ok(tenant)
    }

    pub async fn update_user_self(
        &self,
        user_id: &logisticos_types::UserId,
        cmd: crate::application::commands::UpdateUserSelfCommand,
    ) -> AppResult<User> {
        use validator::Validate;
        cmd.validate().map_err(|e| AppError::Validation(e.to_string()))?;

        let mut user = self.user_repo.find_by_id(user_id).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "User", id: user_id.inner().to_string() })?;

        if let Some(first_name) = cmd.first_name {
            user.first_name = first_name;
        }
        if let Some(last_name) = cmd.last_name {
            user.last_name = last_name;
        }
        if let Some(phone) = cmd.phone_number {
            user.phone_number = if phone.trim().is_empty() {
                None
            } else {
                Some(normalise_phone(&phone))
            };
        }
        user.updated_at = chrono::Utc::now();

        self.user_repo.save(&user).await.map_err(AppError::Internal)?;
        tracing::info!(user_id = %user.id, "User self-updated profile");
        Ok(user)
    }

    /// Set the subscription tier for a tenant. Accepts snake_case tier names
    /// matching `SubscriptionTier`: "starter" | "growth" | "business" | "enterprise".
    /// An audit entry is written by the HTTP handler (not here) so the actor
    /// context (claims) is preserved without threading it through the service.
    pub async fn upgrade_tier(
        &self,
        tenant_id: &logisticos_types::TenantId,
        cmd: crate::application::commands::UpgradeTierCommand,
    ) -> AppResult<logisticos_types::SubscriptionTier> {
        use validator::Validate;
        cmd.validate().map_err(|e| AppError::Validation(e.to_string()))?;

        let new_tier = match cmd.tier.as_str() {
            "starter"    => logisticos_types::SubscriptionTier::Starter,
            "growth"     => logisticos_types::SubscriptionTier::Growth,
            "business"   => logisticos_types::SubscriptionTier::Business,
            "enterprise" => logisticos_types::SubscriptionTier::Enterprise,
            other => return Err(AppError::Validation(format!(
                "Unknown tier '{}'. Valid values: starter, growth, business, enterprise", other
            ))),
        };

        let mut tenant = self.tenant_repo
            .find_by_id(tenant_id).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "Tenant",
                id: tenant_id.inner().to_string(),
            })?;

        tenant.upgrade_tier(new_tier);
        self.tenant_repo.save(&tenant).await.map_err(AppError::Internal)?;

        tracing::info!(tenant_id = %tenant.id, tier = %cmd.tier, "Tenant tier updated");
        Ok(new_tier)
    }

    pub async fn invite_user(&self, tenant_id: &logisticos_types::TenantId, cmd: InviteUserCommand) -> AppResult<(User, String)> {
        // Verify tenant is active before allowing user creation
        let tenant = self.tenant_repo.find_by_id(tenant_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Tenant", id: tenant_id.inner().to_string() })?;

        if !tenant.is_active {
            return Err(AppError::BusinessRule("Cannot invite users to a suspended tenant".into()));
        }

        // Drivers MUST be onboarded with a phone number — the Driver App logs
        // in via SMS OTP and resolves the pre-registered user by phone (see
        // AuthService::otp_verify). Without it, OTP login falls through to
        // the synthetic-email auto-create path and creates a shadow user
        // whose driver-ops profile stays disconnected from the real onboarded
        // row — the driver appears online in their app but invisible to
        // dispatch. Reject the invite at source instead of leaving ops to
        // diagnose the symptom in production.
        let is_driver = cmd.roles.iter().any(|r| r == "driver");
        let has_phone = cmd.phone_number.as_deref().map(|p| !p.trim().is_empty()).unwrap_or(false);
        if is_driver && !has_phone {
            return Err(AppError::Validation(
                "phone_number is required when inviting a driver — the Driver App OTP login resolves users by phone".into()
            ));
        }

        // Check for duplicate email within tenant.
        // Idempotent re-invite: if a user with the same email already exists in
        // this tenant and has the same role (e.g. a prior request saved the row
        // but crashed before returning), re-issue a fresh temp password instead
        // of rejecting. This lets the admin modal safely retry without manual
        // DB cleanup. Conflicting roles (different role on existing user) still
        // raise an error.
        if let Some(mut existing) = self.user_repo.find_by_email(tenant_id, &cmd.email).await.map_err(AppError::Internal)? {
            let same_roles = cmd.roles.iter().all(|r| existing.roles.contains(r));
            if !same_roles {
                return Err(AppError::BusinessRule(format!(
                    "User with email '{}' already exists with different roles", cmd.email
                )));
            }
            // Re-issue a fresh temp password so the admin can complete onboarding.
            let temp_password = generate_temp_password();
            let password_hash = hash_password(&temp_password)
                .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
            existing.password_hash = password_hash;
            // Refresh phone if provided (in case it was wrong on the first attempt).
            if let Some(phone) = cmd.phone_number {
                existing.phone_number = Some(normalise_phone(&phone));
            }
            existing.updated_at = chrono::Utc::now();
            self.user_repo.save(&existing).await.map_err(AppError::Internal)?;
            tracing::info!(user_id = %existing.id, "Re-issued temp password for existing user on idempotent invite");
            return Ok((existing, temp_password));
        }

        // Invited users get a temporary password — they should change on first login.
        // In production this would send an email with a magic link; for now generate a secure random one.
        let temp_password = generate_temp_password();
        let password_hash = hash_password(&temp_password)
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

        let mut user = User::new(
            tenant_id.clone(),
            cmd.email.clone(),
            password_hash,
            cmd.first_name,
            cmd.last_name,
            cmd.roles,
        );

        // Normalise and store the phone number when provided (required for
        // Driver App OTP login so the app can resolve this user by phone).
        user.phone_number = cmd.phone_number.map(|p| normalise_phone(&p));

        // Drivers use OTP login (no email verification step). All other
        // invited roles (merchant, admin, partner) can log in immediately
        // with the temp password — mark verified so can_login() passes.
        if !is_driver {
            user.email_verified = true;
        }

        self.user_repo.save(&user).await.map_err(AppError::Internal)?;

        // Emit USER_CREATED so downstream services (e.g. dispatch) can populate caches.
        // Fire-and-forget: a missing Kafka topic must not roll back user creation.
        let event = Event::new(
            "identity",
            "user.created",
            tenant_id.inner(),
            UserCreated {
                user_id:   user.id.inner(),
                tenant_id: tenant_id.inner(),
                email:     user.email.clone(),
                roles:     user.roles.clone(),
            },
        );
        if let Err(e) = self.kafka.publish_event(topics::USER_CREATED, &event).await {
            tracing::warn!(user_id = %user.id, error = %e,
                "Failed to publish USER_CREATED event — user saved, downstream consumers may miss this");
        }
        tracing::info!(user_id = %user.id, roles = ?user.roles, "User invited");

        Ok((user, temp_password))
    }

    /// Generates a signed driver invite link.
    ///
    /// The link embeds the tenant slug, the driver's phone number, and an
    /// HMAC-SHA256 signature so the backend can verify the payload without
    /// a DB lookup on every deep-link open. The token row provides a
    /// single-use guard and an audit trail.
    ///
    /// Environment variable required: `DRIVER_INVITE_HMAC_SECRET`.
    ///
    /// URL format:
    ///   `https://driver.cargomarket.net/join?t=<slug>&p=<phone>&sig=<hmac>`
    pub async fn generate_invite_link(
        &self,
        requesting_tenant_id: &logisticos_types::TenantId,
        user_id: uuid::Uuid,
    ) -> AppResult<GenerateDriverInviteLinkResult> {
        use base64::Engine;
        use hmac::{Hmac, Mac};
        use sha2::{Digest, Sha256};

        // ── 1. Load and validate the user ────────────────────────────────────
        let uid = logisticos_types::UserId::from_uuid(user_id);
        let user = self.user_repo.find_by_id(&uid).await
            .map_err(AppError::Internal)?
            .ok_or(AppError::NotFound { resource: "User", id: user_id.to_string() })?;

        // Tenant isolation: the requested user must belong to the caller's tenant.
        if user.tenant_id.inner() != requesting_tenant_id.inner() {
            return Err(AppError::Forbidden { resource: "user".into() });
        }

        if !user.roles.iter().any(|r| r == "driver") {
            return Err(AppError::BusinessRule(
                "invite links can only be generated for users with the 'driver' role".into(),
            ));
        }

        let phone = user.phone_number.as_deref().ok_or_else(|| {
            AppError::BusinessRule(
                "driver has no phone_number — re-invite with a valid phone before generating a link".into(),
            )
        })?;

        // ── 2. Load tenant for slug ───────────────────────────────────────────
        let tenant = self.tenant_repo.find_by_id(requesting_tenant_id).await
            .map_err(AppError::Internal)?
            .ok_or(AppError::NotFound { resource: "Tenant", id: requesting_tenant_id.inner().to_string() })?;
        let slug = &tenant.slug;

        // ── 3. Compute HMAC-SHA256 sig ────────────────────────────────────────
        // Message: "<slug>:<phone>"  (same pattern as verify_partner_signature)
        let secret = std::env::var("DRIVER_INVITE_HMAC_SECRET")
            .map_err(|_| AppError::Internal(anyhow::anyhow!("DRIVER_INVITE_HMAC_SECRET not set")))?;

        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes())
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        mac.update(slug.as_bytes());
        mac.update(b":");
        mac.update(phone.as_bytes());
        let sig_bytes = mac.finalize().into_bytes();
        let sig = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sig_bytes);

        // ── 4. Persist token row (audit trail + single-use guard) ─────────────
        let token_hash = {
            let mut hasher = Sha256::new();
            hasher.update(sig.as_bytes());
            format!("{:x}", hasher.finalize())
        };
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(72);
        self.driver_invite_token_repo
            .create(uuid::Uuid::new_v4(), requesting_tenant_id.inner(), user_id, &token_hash, expires_at)
            .await
            .map_err(AppError::Internal)?;

        // ── 5. Build URL ──────────────────────────────────────────────────────
        // E.164 phones contain only '+' and ASCII digits. Percent-encode the
        // leading '+' so the query param survives naive URL parsers.
        let phone_encoded = phone.replace('+', "%2B");
        let invite_url = format!(
            "https://driver.cargomarket.net/join?t={slug}&p={phone_encoded}&sig={sig}"
        );

        tracing::info!(
            user_id  = %user_id,
            slug     = %slug,
            expires  = %expires_at,
            "Driver invite link generated"
        );

        Ok(GenerateDriverInviteLinkResult {
            invite_url,
            expires_at: expires_at.to_rfc3339(),
        })
    }
}

/// Generate a cryptographically secure 16-character alphanumeric temporary password.
fn generate_temp_password() -> String {
    use rand::Rng;
    rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(16)
        .map(char::from)
        .collect()
}

/// Normalise a phone number to E.164 form: keep only `+` and digits, strip
/// spaces / dashes / parentheses. E.g. `+63 917 123 4567` → `+639171234567`.
/// If the input has no leading `+`, one is prepended.
pub fn normalise_phone(raw: &str) -> String {
    let digits_and_plus: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '+')
        .collect();

    if digits_and_plus.starts_with('+') {
        digits_and_plus
    } else {
        format!("+{digits_and_plus}")
    }
}

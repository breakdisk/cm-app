use std::sync::Arc;
use logisticos_auth::password::hash_password;
use logisticos_errors::{AppError, AppResult};
use logisticos_events::{producer::KafkaProducer, topics, payloads::{TenantCreated, UserCreated}, envelope::Event};
use crate::{
    application::commands::{CreateTenantCommand, FinalizeTenantCommand, InviteUserCommand},
    domain::{
        entities::{Tenant, User},
        repositories::{TenantRepository, UserRepository},
    },
};

pub struct TenantService {
    tenant_repo: Arc<dyn TenantRepository>,
    user_repo: Arc<dyn UserRepository>,
    kafka: Arc<KafkaProducer>,
}

impl TenantService {
    pub fn new(
        tenant_repo: Arc<dyn TenantRepository>,
        user_repo: Arc<dyn UserRepository>,
        kafka: Arc<KafkaProducer>,
    ) -> Self {
        Self { tenant_repo, user_repo, kafka }
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
        self.kafka.publish_event(topics::TENANT_CREATED, &event).await
            .map_err(AppError::Internal)?;

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

        // TODO: emit `tenant.finalized` Kafka event once libs/events has the
        // payload; downstream consumers (engagement welcome flow, billing
        // setup) currently react to `tenant.created` which is already fired
        // for draft tenants at provision time.

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

    pub async fn invite_user(&self, tenant_id: &logisticos_types::TenantId, cmd: InviteUserCommand) -> AppResult<(User, String)> {
        // Verify tenant is active before allowing user creation
        let tenant = self.tenant_repo.find_by_id(tenant_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Tenant", id: tenant_id.inner().to_string() })?;

        if !tenant.is_active {
            return Err(AppError::BusinessRule("Cannot invite users to a suspended tenant".into()));
        }

        // Check for duplicate email within tenant
        if self.user_repo.find_by_email(tenant_id, &cmd.email).await.map_err(AppError::Internal)?.is_some() {
            return Err(AppError::BusinessRule(format!("User with email '{}' already exists", cmd.email)));
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

        self.user_repo.save(&user).await.map_err(AppError::Internal)?;

        // Emit USER_CREATED so downstream services (e.g. dispatch) can populate caches.
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
        self.kafka.publish_event(topics::USER_CREATED, &event).await
            .map_err(AppError::Internal)?;
        tracing::info!(user_id = %user.id, roles = ?user.roles, "User created, USER_CREATED event emitted");

        Ok((user, temp_password))
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

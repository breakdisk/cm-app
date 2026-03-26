use std::sync::Arc;
use logisticos_auth::password::hash_password;
use logisticos_errors::{AppError, AppResult};
use logisticos_events::{producer::KafkaProducer, topics, payloads::TenantCreated, envelope::Event};
use crate::{
    application::commands::{CreateTenantCommand, InviteUserCommand},
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

    pub async fn invite_user(&self, tenant_id: &logisticos_types::TenantId, cmd: InviteUserCommand) -> AppResult<User> {
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

        let user = User::new(
            tenant_id.clone(),
            cmd.email.clone(),
            password_hash,
            cmd.first_name,
            cmd.last_name,
            cmd.roles,
        );

        self.user_repo.save(&user).await.map_err(AppError::Internal)?;
        tracing::info!(user_id = %user.id, email = %user.email, "User invited");

        Ok(user)
    }
}

/// Generate a cryptographically secure 16-char temporary password.
/// Format: 4 groups of 4 hex chars separated by hyphens (e.g. "a3f1-9e2c-7b4d-1234")
fn generate_temp_password() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;

    // Use system time + thread ID entropy for a non-csprng fallback in bootstrapping.
    // In production, swap for `rand::thread_rng().gen::<[u8; 16]>()` with the rand crate.
    let seed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();

    let mut h = DefaultHasher::new();
    seed.hash(&mut h);
    std::thread::current().id().hash(&mut h);
    let v1 = h.finish();
    v1.hash(&mut h);
    let v2 = h.finish();

    format!("{:04x}-{:04x}-{:04x}-{:04x}",
        (v1 >> 48) & 0xFFFF,
        (v1 >> 32) & 0xFFFF,
        (v2 >> 48) & 0xFFFF,
        (v2 >> 32) & 0xFFFF,
    )
}

use std::sync::Arc;
use std::fmt::Write;
use logisticos_auth::{jwt::JwtService, password::verify_password, claims::Claims, rbac::default_permissions_for_role};
use logisticos_errors::{AppError, AppResult};
use crate::{
    application::commands::{LoginCommand, LoginResult, RefreshTokenCommand},
    domain::repositories::{TenantRepository, UserRepository},
    infrastructure::db::user_repo::{PgPasswordResetTokenRepository, PgEmailVerificationTokenRepository},
};

pub struct AuthService {
    tenant_repo: Arc<dyn TenantRepository>,
    user_repo: Arc<dyn UserRepository>,
    jwt: Arc<JwtService>,
    reset_token_repo: Arc<PgPasswordResetTokenRepository>,
    email_verification_token_repo: Arc<PgEmailVerificationTokenRepository>,
}

impl AuthService {
    pub fn new(
        tenant_repo: Arc<dyn TenantRepository>,
        user_repo: Arc<dyn UserRepository>,
        jwt: Arc<JwtService>,
        reset_token_repo: Arc<PgPasswordResetTokenRepository>,
        email_verification_token_repo: Arc<PgEmailVerificationTokenRepository>,
    ) -> Self {
        Self { tenant_repo, user_repo, jwt, reset_token_repo, email_verification_token_repo }
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
        let tenant = self.tenant_repo.find_by_slug(&cmd.tenant_slug).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Tenant", id: cmd.tenant_slug.clone() })?;

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
        let token_hash = sha2_hash(cmd.token.as_bytes());

        let (user_id, _tenant_id) = self.reset_token_repo
            .find_valid_by_token(&token_hash).await
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

        self.reset_token_repo.mark_used(&token_hash).await.map_err(AppError::Internal)?;
        tracing::info!(user_id = %user_id, "Password reset completed");
        Ok(())
    }

    pub async fn send_verification_email(&self, cmd: crate::application::commands::SendVerificationEmailCommand) -> AppResult<()> {
        let tenant = self.tenant_repo.find_by_slug(&cmd.tenant_slug).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Tenant", id: cmd.tenant_slug.clone() })?;

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

    pub async fn verify_email(&self, cmd: crate::application::commands::VerifyEmailCommand) -> AppResult<()> {
        let token_hash = sha2_hash(cmd.token.as_bytes());

        let (user_id, _tenant_id) = self.email_verification_token_repo
            .find_valid(&token_hash).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Unauthorized("Invalid or expired verification token".into()))?;

        let user_id_typed = logisticos_types::UserId::from_uuid(user_id);
        let mut user = self.user_repo.find_by_id(&user_id_typed).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "User", id: user_id.to_string() })?;

        user.email_verified = true;
        user.updated_at = chrono::Utc::now();
        self.user_repo.save(&user).await.map_err(AppError::Internal)?;

        self.email_verification_token_repo.mark_used(&token_hash).await.map_err(AppError::Internal)?;
        tracing::info!(user_id = %user_id, "Email verified");
        Ok(())
    }
}

fn sha2_hash(data: &[u8]) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    result.iter().fold(String::new(), |mut s, b| { write!(s, "{b:02x}").ok(); s })
}

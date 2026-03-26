use std::sync::Arc;
use logisticos_auth::{jwt::JwtService, password::verify_password, claims::Claims, rbac::default_permissions_for_role};
use logisticos_errors::{AppError, AppResult};
use crate::{
    application::commands::{LoginCommand, LoginResult, RefreshTokenCommand},
    domain::repositories::{TenantRepository, UserRepository},
};

pub struct AuthService {
    tenant_repo: Arc<dyn TenantRepository>,
    user_repo: Arc<dyn UserRepository>,
    jwt: Arc<JwtService>,
}

impl AuthService {
    pub fn new(
        tenant_repo: Arc<dyn TenantRepository>,
        user_repo: Arc<dyn UserRepository>,
        jwt: Arc<JwtService>,
    ) -> Self {
        Self { tenant_repo, user_repo, jwt }
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
}

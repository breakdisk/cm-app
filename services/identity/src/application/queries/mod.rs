//! Read-side queries — returns view models shaped for API responses.
//! Queries bypass the write-model repositories and can read directly from
//! optimized read replicas or denormalized views.

use std::sync::Arc;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;
use crate::domain::{entities::{User, ApiKey}, repositories::{UserRepository, ApiKeyRepository}};

/// Thin query object for user reads — no business logic, just data retrieval.
pub struct UserQueries {
    user_repo: Arc<dyn UserRepository>,
}

impl UserQueries {
    pub fn new(user_repo: Arc<dyn UserRepository>) -> Self { Self { user_repo } }

    pub async fn list_by_tenant(&self, tenant_id: &TenantId) -> AppResult<Vec<User>> {
        self.user_repo.list_by_tenant(tenant_id).await.map_err(AppError::Internal)
    }

    pub async fn get_by_id(&self, tenant_id: &TenantId, user_id: &logisticos_types::UserId) -> AppResult<User> {
        let user = self.user_repo.find_by_id(user_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "User", id: user_id.inner().to_string() })?;

        // Enforce tenant isolation at the query layer as a second check (RLS is the primary)
        if user.tenant_id != *tenant_id {
            return Err(AppError::NotFound { resource: "User", id: user_id.inner().to_string() });
        }

        Ok(user)
    }
}

/// Thin query object for API key reads.
pub struct ApiKeyQueries {
    api_key_repo: Arc<dyn ApiKeyRepository>,
}

impl ApiKeyQueries {
    pub fn new(api_key_repo: Arc<dyn ApiKeyRepository>) -> Self { Self { api_key_repo } }

    pub async fn list_by_tenant(&self, tenant_id: &TenantId) -> AppResult<Vec<ApiKey>> {
        self.api_key_repo.list_by_tenant(tenant_id).await.map_err(AppError::Internal)
    }
}

use async_trait::async_trait;
use logisticos_types::awb::{Awb, AwbError, ChildAwb, ServiceCode, TenantCode};
use thiserror::Error;

/// Domain-layer contract for generating master AWBs.
///
/// Implementations are provided by the infrastructure layer:
/// - `RedisAwbGenerator`    — primary, uses Redis INCR for atomic sequences
/// - `PostgresAwbGenerator` — fallback, uses PostgreSQL `nextval`
///
/// The trait is kept in the domain layer so the application service can depend
/// on the abstraction without coupling to Redis or PostgreSQL directly.
#[async_trait]
pub trait AwbGenerator: Send + Sync {
    /// Allocate the next sequence number and return a fully-formed master AWB.
    async fn next_awb(
        &self,
        tenant_code: &TenantCode,
        service: ServiceCode,
    ) -> Result<Awb, AwbGeneratorError>;
}

/// Generate all child AWBs for a shipment given its master AWB and piece count.
/// Pure function — no I/O required; piece numbers are deterministic from the master.
pub fn generate_child_awbs(master: &Awb, piece_count: u16) -> Result<Vec<ChildAwb>, AwbError> {
    (1..=piece_count)
        .map(|n| ChildAwb::new(master, n))
        .collect()
}

#[derive(Debug, Error)]
pub enum AwbGeneratorError {
    #[error("Redis error: {0}")]
    Redis(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Sequence overflow for tenant {tenant} service {service} — contact platform ops")]
    SequenceOverflow { tenant: String, service: char },

    #[error("AWB construction error: {0}")]
    Awb(#[from] AwbError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use logisticos_types::awb::TenantCode;

    #[test]
    fn generate_child_awbs_correct_count() {
        let tenant = TenantCode::new("PH1").unwrap();
        let master = Awb::generate(&tenant, ServiceCode::Balikbayan, 9012);
        let children = generate_child_awbs(&master, 3).unwrap();
        assert_eq!(children.len(), 3);
        assert_eq!(children[0].piece_number(), 1);
        assert_eq!(children[1].piece_number(), 2);
        assert_eq!(children[2].piece_number(), 3);
    }

    #[test]
    fn generate_child_awbs_all_reference_master() {
        let tenant = TenantCode::new("PH1").unwrap();
        let master = Awb::generate(&tenant, ServiceCode::Balikbayan, 9012);
        let children = generate_child_awbs(&master, 2).unwrap();
        for child in &children {
            assert_eq!(child.master(), master);
        }
    }

    #[test]
    fn generate_child_awbs_single_piece() {
        let tenant = TenantCode::new("SG2").unwrap();
        let master = Awb::generate(&tenant, ServiceCode::Standard, 1);
        let children = generate_child_awbs(&master, 1).unwrap();
        assert_eq!(children.len(), 1);
        assert!(children[0].as_str().ends_with("-001"));
    }
}

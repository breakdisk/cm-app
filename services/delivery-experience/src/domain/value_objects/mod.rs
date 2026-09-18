//! Who may read a tracking record.
//!
//! The authenticated view carries the driver's phone number and live position.
//! It used to check the tenant only, so any customer holding `shipments:read`
//! could read any other customer's record by shipment id, and `GET /v1/tracking`
//! listed the whole tenant's. order-intake had the same hole and closed it with
//! the same rule (`logisticos_auth::rbac::shipments_tenant_wide`).

use uuid::Uuid;

/// The caller, as far as reading tracking goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reader {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    /// True when the caller reaches every shipment in their tenant. False when
    /// they reach only the shipments they booked.
    pub tenant_wide: bool,
}

impl Reader {
    /// The owner to filter a list by: none for a tenant-wide reader, the
    /// caller themselves for everyone else.
    pub fn owner_filter(&self) -> Option<Uuid> {
        if self.tenant_wide { None } else { Some(self.user_id) }
    }
}

/// Tenant is always enforced. An owner-scoped reader also needs the record's
/// owner to be them — and a record with no owner, projected before the owner
/// was carried, is refused rather than guessed at.
pub fn may_read(reader: &Reader, record_tenant: Uuid, record_owner: Option<Uuid>) -> bool {
    if reader.tenant_id != record_tenant {
        return false;
    }
    reader.tenant_wide || record_owner == Some(reader.user_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> (Uuid, Uuid, Uuid) {
        (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
    }

    /// The live bug: a customer reading another customer's record, driver
    /// phone included, by shipment id.
    #[test]
    fn a_customer_reads_only_their_own() {
        let (t, me, other) = ids();
        let customer = Reader { tenant_id: t, user_id: me, tenant_wide: false };
        assert!(may_read(&customer, t, Some(me)));
        assert!(!may_read(&customer, t, Some(other)));
    }

    #[test]
    fn another_tenant_is_refused_even_for_an_operator() {
        let (t1, t2, op) = ids();
        let operator = Reader { tenant_id: t1, user_id: op, tenant_wide: true };
        assert!(!may_read(&operator, t2, Some(op)));
    }

    #[test]
    fn an_operator_reads_anything_in_their_tenant() {
        let (t, op, owner) = ids();
        let operator = Reader { tenant_id: t, user_id: op, tenant_wide: true };
        assert!(may_read(&operator, t, Some(owner)));
        assert!(may_read(&operator, t, None));
    }

    /// Records projected before the owner was carried have none. Failing
    /// closed costs a customer the signed-in view of an old shipment; the
    /// public tracking-number page still works for them.
    #[test]
    fn a_record_with_no_owner_is_refused_to_an_owner_scoped_reader() {
        let (t, me, _) = ids();
        let customer = Reader { tenant_id: t, user_id: me, tenant_wide: false };
        assert!(!may_read(&customer, t, None));
    }

    #[test]
    fn a_list_is_filtered_to_the_caller_unless_they_are_tenant_wide() {
        let (t, me, _) = ids();
        assert_eq!(Reader { tenant_id: t, user_id: me, tenant_wide: false }.owner_filter(), Some(me));
        assert_eq!(Reader { tenant_id: t, user_id: me, tenant_wide: true }.owner_filter(), None);
    }
}

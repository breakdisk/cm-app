//! The whole-home moving catalogue: platform defaults, overridden per tenant
//! one group at a time.

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::value_objects::home_move::CatalogueItem;

/// The tenant's catalogue: for each group, the tenant's own rows when it has
/// any, else the platform defaults. A tenant that re-prices its bedroom
/// furniture does not thereby lose the kitchen.
pub fn merge(defaults: Vec<CatalogueItem>, tenant: Vec<CatalogueItem>) -> Vec<CatalogueItem> {
    let overridden: std::collections::HashSet<String> = tenant.iter().map(|i| i.group.clone()).collect();
    defaults
        .into_iter()
        .filter(|i| !overridden.contains(&i.group))
        .chain(tenant)
        .collect()
}

pub async fn catalogue_for(pool: &PgPool, tenant_id: Uuid) -> anyhow::Result<Vec<CatalogueItem>> {
    let rows = sqlx::query(
        r#"SELECT tenant_id, group_key, item_key, name, volume_l, weight_kg, assembly, packing
             FROM order_intake.home_catalogue
            WHERE active AND (tenant_id IS NULL OR tenant_id = $1)
            ORDER BY group_key, sort_order, name"#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    let (mut defaults, mut own) = (Vec::new(), Vec::new());
    for r in rows {
        let item = CatalogueItem {
            key: r.get("item_key"),
            group: r.get("group_key"),
            name: r.get("name"),
            volume_l: r.get("volume_l"),
            weight_kg: r.get("weight_kg"),
            assembly: r.get("assembly"),
            packing: r.get("packing"),
        };
        if r.get::<Option<Uuid>, _>("tenant_id").is_some() {
            own.push(item);
        } else {
            defaults.push(item);
        }
    }
    Ok(merge(defaults, own))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(group: &str, key: &str, volume_l: i32) -> CatalogueItem {
        CatalogueItem { key: key.into(), group: group.into(), name: key.into(), volume_l, weight_kg: 10, assembly: false, packing: false }
    }

    #[test]
    fn a_tenant_group_replaces_only_that_group() {
        let merged = merge(
            vec![item("bed", "king_bed", 2_600), item("bed", "single_bed", 1_200), item("kitchen", "fridge", 1_100)],
            vec![item("bed", "king_bed", 3_000)],
        );
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|i| i.key == "fridge"));
        assert_eq!(merged.iter().find(|i| i.key == "king_bed").map(|i| i.volume_l), Some(3_000));
        assert!(!merged.iter().any(|i| i.key == "single_bed"), "the tenant's bed group is the whole bed group");
    }
}

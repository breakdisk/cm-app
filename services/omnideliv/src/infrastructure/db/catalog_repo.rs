use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{Availability, AvailabilityState, CatalogItem};
use crate::domain::repositories::{CatalogRepository, ItemWithAvailability};

pub struct PgCatalogRepository { pool: PgPool }

impl PgCatalogRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn map_pair(r: &sqlx::postgres::PgRow) -> anyhow::Result<ItemWithAvailability> {
    let state_str: String = r.get("state");
    let state = match state_str.as_str() {
        "available"    => AvailabilityState::Available,
        "limited"      => AvailabilityState::Limited,
        "out_of_stock" => AvailabilityState::OutOfStock,
        other => anyhow::bail!("unknown availability state in database: {other}"),
    };

    let item = CatalogItem {
        id:             r.get("id"),
        tenant_id:      r.get("tenant_id"),
        vendor_id:      r.get("vendor_id"),
        sku:            r.get("sku"),
        name:           r.get("name"),
        description:    r.get("description"),
        price_cents:    r.get("price_cents"),
        modifiers:      r.get("modifiers"),
        allergens:      r.get("allergens"),
        dietary_tags:   r.get("dietary_tags"),
        vertical_attrs: r.get("vertical_attrs"),
        is_listed:      r.get("is_listed"),
        created_at:     r.get("created_at"),
        updated_at:     r.get("updated_at"),
    };

    let availability = Availability {
        item_id:    item.id,
        tenant_id:  item.tenant_id,
        state,
        updated_at: r.get("availability_updated_at"),
        updated_by: r.get("updated_by"),
    };

    Ok(ItemWithAvailability { item, availability })
}

#[async_trait]
impl CatalogRepository for PgCatalogRepository {
    async fn save_item(&self, i: &CatalogItem) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO omnideliv.catalog_items (
                id, tenant_id, vendor_id, sku, name, description, price_cents,
                modifiers, allergens, dietary_tags, vertical_attrs, is_listed,
                created_at, updated_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
            ON CONFLICT (id) DO UPDATE SET
                name           = EXCLUDED.name,
                description    = EXCLUDED.description,
                price_cents    = EXCLUDED.price_cents,
                modifiers      = EXCLUDED.modifiers,
                allergens      = EXCLUDED.allergens,
                dietary_tags   = EXCLUDED.dietary_tags,
                vertical_attrs = EXCLUDED.vertical_attrs,
                is_listed      = EXCLUDED.is_listed,
                updated_at     = EXCLUDED.updated_at
            "#,
        )
        .bind(i.id).bind(i.tenant_id).bind(i.vendor_id)
        .bind(&i.sku).bind(&i.name).bind(&i.description).bind(i.price_cents)
        .bind(&i.modifiers).bind(&i.allergens).bind(&i.dietary_tags)
        .bind(&i.vertical_attrs).bind(i.is_listed)
        .bind(i.created_at).bind(i.updated_at)
        .execute(&mut *tx).await?;

        // A new item is available by default — but the freshness stamp starts
        // now, so it is honestly "declared present just now" rather than
        // silently inheriting trust it has not earned.
        sqlx::query(
            r#"
            INSERT INTO omnideliv.item_availability (item_id, tenant_id, state, updated_at)
            VALUES ($1, $2, 'available', NOW())
            ON CONFLICT (item_id) DO NOTHING
            "#,
        )
        .bind(i.id).bind(i.tenant_id)
        .execute(&mut *tx).await?;

        tx.commit().await?;
        Ok(())
    }

    async fn set_availability(&self, a: &Availability) -> anyhow::Result<()> {
        // updated_at is set to NOW() server-side rather than trusting the
        // caller's clock — the freshness stamp is only meaningful if it records
        // when the declaration actually reached us.
        sqlx::query(
            r#"
            INSERT INTO omnideliv.item_availability (item_id, tenant_id, state, updated_at, updated_by)
            VALUES ($1, $2, $3, NOW(), $4)
            ON CONFLICT (item_id) DO UPDATE SET
                state      = EXCLUDED.state,
                updated_at = NOW(),
                updated_by = EXCLUDED.updated_by
            "#,
        )
        .bind(a.item_id).bind(a.tenant_id).bind(a.state.as_str()).bind(a.updated_by)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn list_for_vendor(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
    ) -> anyhow::Result<Vec<ItemWithAvailability>> {
        let rows = sqlx::query(
            r#"
            SELECT i.*, a.state, a.updated_at AS availability_updated_at, a.updated_by
              FROM omnideliv.catalog_items i
              JOIN omnideliv.item_availability a ON a.item_id = i.id
             WHERE i.tenant_id = $1 AND i.vendor_id = $2 AND i.is_listed
             ORDER BY i.name
            "#,
        )
        .bind(tenant_id).bind(vendor_id)
        .fetch_all(&self.pool).await?;

        rows.iter().map(map_pair).collect()
    }

    async fn search(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        query: &str,
        avoid_allergens: &[String],
        limit: i64,
    ) -> anyhow::Result<Vec<ItemWithAvailability>> {
        // Allergen exclusion uses && (array overlap) against the GIN index.
        // Out-of-stock items are deliberately NOT filtered out here — the
        // Nutritionist needs to see them to propose a substitute, and hiding
        // them would make "we swapped X for Y" impossible to explain.
        let rows = sqlx::query(
            r#"
            SELECT i.*, a.state, a.updated_at AS availability_updated_at, a.updated_by
              FROM omnideliv.catalog_items i
              JOIN omnideliv.item_availability a ON a.item_id = i.id
             WHERE i.tenant_id = $1
               AND i.vendor_id = $2
               AND i.is_listed
               AND (i.name ILIKE '%' || $3 || '%' OR i.description ILIKE '%' || $3 || '%')
               AND NOT (i.allergens && $4::TEXT[])
             ORDER BY i.name
             LIMIT $5
            "#,
        )
        .bind(tenant_id).bind(vendor_id).bind(query).bind(avoid_allergens).bind(limit)
        .fetch_all(&self.pool).await?;

        rows.iter().map(map_pair).collect()
    }
}

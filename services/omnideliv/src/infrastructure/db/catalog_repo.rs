use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{
    Availability, AvailabilityState, CatalogItem, CatalogSource, ModifierGroup,
};
use crate::domain::repositories::{CatalogRepository, ItemFacts, ItemWithAvailability};

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
        modifiers:      r.get::<sqlx::types::Json<Vec<ModifierGroup>>, _>("modifiers").0,
        allergens:      r.get("allergens"),
        allergens_declared_at: r.get("allergens_declared_at"),
        dietary_tags:   r.get("dietary_tags"),
        category:       r.get("category"),
        vertical_attrs: r.get("vertical_attrs"),
        is_listed:      r.get("is_listed"),
        // An unknown source in the database is a schema/CHECK drift, not a
        // recoverable row — refuse rather than silently relabel it manual.
        source:         {
            let s: String = r.get("source");
            CatalogSource::parse(&s)
                .ok_or_else(|| anyhow::anyhow!("unknown catalog source in database: {s}"))?
        },
        external_id:    r.get("external_id"),
        synced_at:      r.get("synced_at"),
        image_key:      r.get("image_key"),
        created_at:     r.get("created_at"),
        updated_at:     r.get("updated_at"),
    };

    let availability = Availability {
        item_id:    item.id,
        tenant_id:  item.tenant_id,
        state,
        updated_at:   r.get("availability_updated_at"),
        confirmed_at: r.get("confirmed_at"),
        updated_by:   r.get("updated_by"),
    };

    Ok(ItemWithAvailability { item, availability })
}

#[async_trait]
impl CatalogRepository for PgCatalogRepository {
    async fn save_item(&self, i: &CatalogItem) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;

        // Bound through a local named for its column. Inline as
        // `sqlx::types::Json(&i.modifiers)` the argument reads as `types` to
        // check-sql-bind-arity.py, which would make it skip this statement —
        // the exact statement whose bind order broke every catalog write.
        let modifiers = sqlx::types::Json(&i.modifiers);

        sqlx::query(
            r#"
            INSERT INTO omnideliv.catalog_items (
                id, tenant_id, vendor_id, sku, name, description, price_cents,
                modifiers, allergens, dietary_tags, vertical_attrs, is_listed,
                source, external_id, synced_at, allergens_declared_at,
                created_at, updated_at, category
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
            ON CONFLICT (id) DO UPDATE SET
                sku            = EXCLUDED.sku,
                name           = EXCLUDED.name,
                description    = EXCLUDED.description,
                price_cents    = EXCLUDED.price_cents,
                modifiers      = EXCLUDED.modifiers,
                allergens      = EXCLUDED.allergens,
                dietary_tags   = EXCLUDED.dietary_tags,
                category       = EXCLUDED.category,
                vertical_attrs = EXCLUDED.vertical_attrs,
                is_listed      = EXCLUDED.is_listed,
                source         = EXCLUDED.source,
                external_id    = EXCLUDED.external_id,
                synced_at      = EXCLUDED.synced_at,
                -- Written from the entity, which applies the merge rules. An
                -- ingest never advances it; only `declare_allergens` does.
                allergens_declared_at = EXCLUDED.allergens_declared_at,
                -- image_key is absent on purpose. Only the photo endpoint
                -- writes it, so re-syncing a Shopify or CSV catalog cannot
                -- wipe a picture the vendor uploaded by hand.
                updated_at     = EXCLUDED.updated_at
            "#,
        )
        .bind(i.id).bind(i.tenant_id).bind(i.vendor_id)
        .bind(&i.sku).bind(&i.name).bind(&i.description).bind(i.price_cents)
        .bind(modifiers).bind(&i.allergens).bind(&i.dietary_tags)
        .bind(&i.vertical_attrs).bind(i.is_listed)
        .bind(i.source.as_str()).bind(&i.external_id).bind(i.synced_at)
        .bind(i.allergens_declared_at)
        // Positional. `category` is last in the column list above, so it binds
        // last — putting it here rather than next to its neighbours is not
        // style, it is the contract. Binding it before created_at shifted every
        // parameter after it and Postgres rejected the whole write with
        // `column "created_at" is of type timestamp with time zone but
        // expression is of type text` — which broke *every* catalog write, not
        // just the ones setting a category.
        .bind(i.created_at).bind(i.updated_at)
        .bind(&i.category)
        .execute(&mut *tx).await?;

        // A new item is listed as available — but `confirmed_at` stays NULL, so
        // it reads as uncertain until a human taps it. This is what stops a bulk
        // import from presenting 200 unverified items to the agent as fact.
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

    async fn find_items(&self, tenant_id: Uuid, ids: &[Uuid]) -> anyhow::Result<Vec<CatalogItem>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            r#"
            SELECT i.*, a.state, a.updated_at AS availability_updated_at,
                   a.confirmed_at, a.updated_by
              FROM omnideliv.catalog_items i
              JOIN omnideliv.item_availability a ON a.item_id = i.id
             WHERE i.tenant_id = $1 AND i.id = ANY($2)
            "#,
        )
        .bind(tenant_id).bind(ids)
        .fetch_all(&self.pool).await?;

        rows.iter().map(|r| map_pair(r).map(|p| p.item)).collect()
    }

    async fn find_item(&self, tenant_id: Uuid, item_id: Uuid) -> anyhow::Result<Option<CatalogItem>> {
        // Joins availability so the row shape matches `map_pair`; the caller
        // only wants the item, but one mapper is better than two that can drift.
        let row = sqlx::query(
            r#"
            SELECT i.*, a.state, a.updated_at AS availability_updated_at,
                   a.confirmed_at, a.updated_by
              FROM omnideliv.catalog_items i
              JOIN omnideliv.item_availability a ON a.item_id = i.id
             WHERE i.tenant_id = $1 AND i.id = $2
            "#,
        )
        .bind(tenant_id).bind(item_id)
        .fetch_optional(&self.pool).await?;

        row.as_ref().map(map_pair).transpose().map(|o| o.map(|p| p.item))
    }

    async fn set_image_key(
        &self,
        tenant_id: Uuid,
        item_id:   Uuid,
        key:       Option<&str>,
    ) -> anyhow::Result<()> {
        // Filtered on tenant as well as id: an item id is a bare UUID from the
        // request path, and this is a write.
        sqlx::query(
            "UPDATE omnideliv.catalog_items
                SET image_key = $3, updated_at = NOW()
              WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant_id)
        .bind(item_id)
        .bind(key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn set_availability(&self, a: &Availability) -> anyhow::Result<()> {
        // Both clocks are NOW() server-side rather than the caller's — a stamp
        // is only meaningful if it records when the declaration reached us.
        //
        // What the caller *does* decide is whether this was a human act: a
        // `confirmed_at` of Some means "a person stated this", and only then
        // does the attestation clock move. An ingest passes None and the
        // existing confirmation is left exactly where it was.
        let is_human = a.confirmed_at.is_some();
        sqlx::query(
            r#"
            INSERT INTO omnideliv.item_availability
                   (item_id, tenant_id, state, updated_at, confirmed_at, updated_by)
            VALUES ($1, $2, $3, NOW(), CASE WHEN $5 THEN NOW() END, $4)
            ON CONFLICT (item_id) DO UPDATE SET
                state        = EXCLUDED.state,
                updated_at   = NOW(),
                confirmed_at = CASE WHEN $5 THEN NOW()
                                    ELSE omnideliv.item_availability.confirmed_at END,
                updated_by   = COALESCE(EXCLUDED.updated_by, omnideliv.item_availability.updated_by)
            "#,
        )
        .bind(a.item_id).bind(a.tenant_id).bind(a.state.as_str()).bind(a.updated_by)
        .bind(is_human)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn find_item_by_sku(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        sku: &str,
    ) -> anyhow::Result<Option<CatalogItem>> {
        let row = sqlx::query(
            r#"
            SELECT i.*, a.state, a.updated_at AS availability_updated_at,
                   a.confirmed_at, a.updated_by
              FROM omnideliv.catalog_items i
              JOIN omnideliv.item_availability a ON a.item_id = i.id
             WHERE i.tenant_id = $1 AND i.vendor_id = $2 AND i.sku = $3
            "#,
        )
        .bind(tenant_id).bind(vendor_id).bind(sku)
        .fetch_optional(&self.pool).await?;

        row.as_ref().map(map_pair).transpose().map(|o| o.map(|p| p.item))
    }

    async fn find_item_by_external(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        source: CatalogSource,
        external_id: &str,
    ) -> anyhow::Result<Option<CatalogItem>> {
        let row = sqlx::query(
            r#"
            SELECT i.*, a.state, a.updated_at AS availability_updated_at,
                   a.confirmed_at, a.updated_by
              FROM omnideliv.catalog_items i
              JOIN omnideliv.item_availability a ON a.item_id = i.id
             WHERE i.tenant_id = $1 AND i.vendor_id = $2
               AND i.source = $3 AND i.external_id = $4
            "#,
        )
        .bind(tenant_id).bind(vendor_id).bind(source.as_str()).bind(external_id)
        .fetch_optional(&self.pool).await?;

        row.as_ref().map(map_pair).transpose().map(|o| o.map(|p| p.item))
    }

    async fn confirm_all_for_vendor(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        user_id: Uuid,
    ) -> anyhow::Result<u64> {
        // Scoped by a join on the item's vendor rather than a list of ids from
        // the client: an id list is an id list the caller can extend, and this
        // statement writes an attestation in someone's name.
        //
        // Only `available` rows. A vendor confirming their store is saying "what
        // is listed is on the shelf" — silently flipping their out-of-stock
        // markers back to available would be the opposite of an attestation.
        let res = sqlx::query(
            r#"
            UPDATE omnideliv.item_availability a
               SET confirmed_at = NOW(), updated_at = NOW(), updated_by = $3
              FROM omnideliv.catalog_items i
             WHERE i.id = a.item_id
               AND i.tenant_id = $1 AND i.vendor_id = $2 AND i.is_listed
               AND a.state = 'available'
            "#,
        )
        .bind(tenant_id).bind(vendor_id).bind(user_id)
        .execute(&self.pool).await?;
        Ok(res.rows_affected())
    }

    async fn list_for_vendor(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
    ) -> anyhow::Result<Vec<ItemWithAvailability>> {
        let rows = sqlx::query(
            r#"
            SELECT i.*, a.state, a.updated_at AS availability_updated_at,
                   a.confirmed_at, a.updated_by
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
            SELECT i.*, a.state, a.updated_at AS availability_updated_at,
                   a.confirmed_at, a.updated_by
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

    async fn declare_allergens(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
        allergens: &[String],
    ) -> anyhow::Result<bool> {
        // NOW() server-side, like the availability stamp: an attestation is
        // only meaningful if it records when it reached us, not when a client
        // claims it was made.
        let res = sqlx::query(
            r#"
            UPDATE omnideliv.catalog_items
               SET allergens = $3, allergens_declared_at = NOW()
             WHERE tenant_id = $1 AND id = $2
            "#,
        )
        .bind(tenant_id).bind(item_id).bind(allergens)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    async fn item_facts(
        &self,
        tenant_id: Uuid,
        item_ids: &[Uuid],
    ) -> anyhow::Result<Vec<ItemFacts>> {
        if item_ids.is_empty() {
            return Ok(Vec::new());
        }

        // `vertical` and `prep_time_minutes` live on the vendor, not the item,
        // so this joins rather than reading catalog_items alone. INNER JOIN:
        // an item whose vendor is missing cannot be classified, and reconcile
        // must treat it as unverifiable rather than guess a temperature class.
        //
        // No `is_listed` filter. A delisted item still has real allergens, and
        // the caller's job here is to verify what was proposed, not to decide
        // whether it is orderable — hiding it would turn a known allergen into
        // an unresolved id, which reads the same but for the wrong reason.
        let rows = sqlx::query(
            r#"
            SELECT i.id, i.allergens, i.price_cents,
                   (i.allergens_declared_at IS NOT NULL) AS allergens_declared,
                   v.vertical, v.prep_time_minutes
              FROM omnideliv.catalog_items i
              JOIN omnideliv.vendors v ON v.id = i.vendor_id AND v.tenant_id = i.tenant_id
             WHERE i.tenant_id = $1
               AND i.id = ANY($2)
            "#,
        )
        .bind(tenant_id)
        .bind(item_ids)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| ItemFacts {
                item_id:           r.get("id"),
                allergens:         r.get("allergens"),
                allergens_declared: r.get("allergens_declared"),
                vertical:          r.get("vertical"),
                prep_time_minutes: r.get("prep_time_minutes"),
                price_cents:       r.get("price_cents"),
            })
            .collect())
    }
}

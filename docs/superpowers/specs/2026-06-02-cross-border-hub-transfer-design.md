# Cross-Border Hub Transfer Design

**Date:** 2026-06-02  
**Status:** Approved  
**Author:** Principal Software Architect  
**Services affected:** hub-ops (primary), order-intake, dispatch, carrier, payments, engagement, libs/types

---

## 1. Problem Statement

LogisticOS has a well-modelled Container/Pallet domain and a bin-pack consolidation engine, but the hub **operations flow** — inbound scan, warehouse placement, consolidation, cross-border transit, customs gate, destination inbound, break-bulk, local sort, and last-mile handoff — is not implemented. The `hub-ops` application handlers and commands are stubs.

This spec fills that gap for the full **bidirectional cross-border** flow: outbound (origin country → abroad) and inbound (abroad → destination country), sharing symmetric event contracts.

---

## 2. Architecture Decision

**Option chosen: hub-ops as event emitter, downstream services react (Event-First).**

Hub-ops owns the full Container/Pallet/Scan/Inventory domain and all state machines. At every milestone transition it emits a Kafka event. Downstream services consume and react independently. No cross-service gRPC calls in the critical path; hub-ops has zero outbound service dependencies.

This matches the existing architecture (status_consumer.rs, delivery_failed_consumer, OutboundSyncWorker patterns).

---

## 3. Domain Model

### 3.1 Already Built — Do Not Rebuild

| Entity | Location | Notes |
|---|---|---|
| `Container` | `services/hub-ops/src/domain/entities/container.rs` | State machine exists; needs 5 new transition methods |
| `ContainerStatus` | `libs/types/src/lib.rs:251` | Missing `Deconsolidated` variant |
| `TransportMode` | `libs/types/src/lib.rs:273` | Road, SeaFcl, SeaLcl, AirUld, AirLoose — complete |
| `Pallet` | `services/hub-ops/src/domain/entities/pallet.rs` | Complete |
| `ConsolidationPlan` | `services/hub-ops/src/domain/entities/consolidation.rs` | Complete |
| `TruckSpec` | same | FCL20, FCL40, road trucks — complete |
| `ShipmentStatus::CustomsHold` | `libs/types/src/lib.rs:207` | Typed but not wired |
| `PieceStatus` | `libs/types/src/lib.rs:215` | Complete |

### 3.2 New Entities (all in `services/hub-ops`)

#### `HubScan` — append-only scan log

Every barcode read at a hub creates one immutable row. This is the chain-of-custody audit trail at hub boundaries. Never updated or deleted.

```rust
pub struct HubScan {
    pub id:               Uuid,
    pub tenant_id:        TenantId,
    pub hub_id:           HubId,
    pub piece_awb:        ChildAwb,
    pub master_awb:       Awb,
    pub shipment_id:      ShipmentId,
    pub scan_type:        ScanType,
    pub scanned_by:       Uuid,               // hub agent or driver_id
    pub device_timestamp: DateTime<Utc>,      // hardware clock at scan moment
    pub server_timestamp: DateTime<Utc>,      // backend receipt time
    pub pallet_id:        Option<PalletId>,
    pub container_id:     Option<ContainerId>,
    pub exception:        Option<ScanException>,
}

pub enum ScanType {
    InboundReceive,          // piece arrives at hub from first-mile driver
    PalletAssign,            // piece scanned onto a pallet
    OutboundLoad,            // pallet or piece loaded into container/vehicle
    ContainerDeconsolidate,  // piece broken out of container at destination hub
    LocalSortAssign,         // piece scanned into last-mile delivery cage/bin
    ExceptionFlag,           // damaged, missing, weight mismatch
}

pub enum ScanException {
    Missing,
    Damaged,
    WeightMismatch,
}
```

Table: `hub_ops.hub_scans` — append-only. `REVOKE UPDATE, DELETE ON hub_ops.hub_scans FROM app_role`.

#### `HubLocation` — warehouse topology tree

Models the structural location hierarchy inside a hub. Ops creates these via admin portal; they are tenant-scoped. Do not store location as free text strings on Shipment or Container rows.

```rust
pub struct HubLocation {
    pub id:            Uuid,
    pub tenant_id:     TenantId,
    pub hub_id:        HubId,
    pub zone_id:       String,           // e.g. "ZONE_A_INBOUND", "ZONE_C_CROSSDOCK"
    pub aisle:         Option<String>,   // e.g. "Aisle 12"
    pub rack:          Option<String>,   // e.g. "Bay 04"
    pub shelf:         Option<String>,   // e.g. "Level 3"
    pub bin:           Option<String>,   // e.g. "Bin B"
    pub location_tag:  String,           // globally unique scannable tag e.g. "HUB1-Z-A12-B04-L3-BB"
    pub capacity_cbm:  Option<f64>,      // prevents overloading a physical slot
    pub is_active:     bool,
    pub created_at:    DateTime<Utc>,
}
```

#### `HubInventory` — current location ledger

Tracks the exact warehouse coordinates of a `ChildAwb` (piece) or `PalletId` (consolidated unit). A single `location_id` update when a forklift operator scans pallet + shelf. This is the only mutable hub entity — `location_id` is updated in place when inventory moves.

**Piece vs. consolidated tracking rule:** When a piece is assigned to a pallet (`PalletAssign` scan), its `Piece(ChildAwb)` inventory record is superseded — it is soft-deleted (or marked `superseded_by_pallet_id`). A single `Consolidated(PalletId)` record becomes the authoritative location for all pieces on that pallet. This means moving a pallet (e.g., from staging to rack to container) requires exactly **one** `HubInventory` update regardless of piece count. At break-bulk (`ContainerDeconsolidate`), individual `Piece(ChildAwb)` records are re-created from pallet records and the `Consolidated` record is archived.

```rust
pub struct HubInventory {
    pub id:             Uuid,
    pub tenant_id:      TenantId,
    pub hub_id:         HubId,
    pub location_id:    Uuid,               // → HubLocation
    pub inventory_unit: InventoryUnit,
    pub batch_number:   Option<String>,     // for batch expiry/clearance handling
    pub checked_in_by:  Uuid,
    pub checked_in_at:  DateTime<Utc>,
    pub updated_at:     DateTime<Utc>,
}

pub enum InventoryUnit {
    Piece(ChildAwb),
    Consolidated(PalletId),
}
```

#### `HubTransferManifest` — customs/freight document

One per container. Holds the carrier booking reference (MAWB or Bill of Lading), customs broker reference, duty amounts, and clearance audit trail.

```rust
pub struct HubTransferManifest {
    pub id:                  Uuid,
    pub tenant_id:           TenantId,
    pub container_id:        ContainerId,
    pub origin_hub_id:       HubId,
    pub destination_hub_id:  HubId,
    pub transport_mode:      TransportMode,
    pub carrier_booking_ref: Option<String>,           // MAWB / Bill of Lading
    pub customs_filing_ref:  Option<String>,           // customs broker reference
    pub customs_status:      CustomsStatus,
    pub duties_total_cents:  Option<i64>,
    pub broker_payload:      Option<serde_json::Value>, // optional broker API pre-fill
    pub cleared_by:          Option<Uuid>,             // hub agent who approved
    pub cleared_at:          Option<DateTime<Utc>>,
    pub created_at:          DateTime<Utc>,
    pub updated_at:          DateTime<Utc>,
}

pub enum CustomsStatus {
    NotRequired,  // domestic road moves
    Pending,      // filed, awaiting inspection
    Hold,         // under customs hold
    Cleared,      // approved and released
}
```

#### `HubRoutingConfig` — last-mile routing rules

Tenant + hub-level config that decides own-driver vs. 3PL at container release time. Keyed by `(tenant_id, hub_id, destination_zone)`.

```rust
pub struct HubRoutingConfig {
    pub id:               Uuid,
    pub tenant_id:        TenantId,
    pub hub_id:           HubId,
    pub destination_zone: String,          // e.g. "Metro Manila", "Visayas"
    pub routing_type:     RoutingType,
    pub carrier_id:       Option<Uuid>,    // which 3PL for Carrier mode
    pub auto_fallback_window_mins: i32,    // Auto mode: minutes before falling back to 3PL
    pub created_at:       DateTime<Utc>,
    pub updated_at:       DateTime<Utc>,
}

pub enum RoutingType {
    OwnDriver,  // always use own dispatch
    Carrier,    // always use 3PL
    Auto,       // try own driver, fall back to carrier after window
}
```

### 3.3 Container Entity Extensions

Add five transition methods to the existing `Container` in `container.rs`. The existing `arrive()` (InTransit → Delivered) is **deprecated** for new flows — `Delivered` is retired as a Container terminal state. `Deconsolidated` is the new terminal.

```rust
// Sea/air only: InTransit → ArrivedAtPort
pub fn arrive_at_port(&mut self) -> Result<(), ContainerError>

// ArrivedAtPort → Customs
pub fn enter_customs(&mut self) -> Result<(), ContainerError>

// Customs → Released (records cleared_by in HubTransferManifest — handled by service layer)
pub fn clear_customs(&mut self) -> Result<(), ContainerError>

// Road mode only: InTransit → Released (skips ArrivedAtPort/Customs)
pub fn release_domestic(&mut self) -> Result<(), ContainerError>

// Released → Deconsolidated; returns all ChildAwbs for routing fan-out
pub fn deconsolidate(&mut self) -> Result<Vec<ChildAwb>, ContainerError> {
    if self.status != ContainerStatus::Released {
        return Err(ContainerError::InvalidTransition {
            from: format!("{:?}", self.status),
            to:   "Deconsolidated".to_string(),
        });
    }
    self.status = ContainerStatus::Deconsolidated;
    self.updated_at = Utc::now();
    let pieces: Vec<ChildAwb> = self.loose_pieces.clone();
    // pallet pieces resolved by service layer from pallet records
    Ok(pieces)
}
```

### 3.4 libs/types Changes

Add `Deconsolidated` to `ContainerStatus`:

```rust
pub enum ContainerStatus {
    Planning,
    Manifested,
    Loading,
    Sealed,
    InTransit,
    ArrivedAtPort,
    Customs,
    Released,
    Deconsolidated,  // NEW — terminal state; replaces Delivered for container lifecycle
    Delivered,       // DEPRECATED — retained for backward compat only
}
```

---

## 4. Full Lifecycle Flow

### 4.1 Container State Machine

```
Domestic road:
  Planning → Manifested → Sealed → InTransit
           → Released → Deconsolidated

Cross-border sea/air:
  Planning → Manifested → Sealed → InTransit
           → ArrivedAtPort → Customs → Released → Deconsolidated
```

### 4.2 Origin Hub — Outbound

```
1. First-mile driver arrives at origin hub
        ↓
2. Hub agent scans each piece barcode
   ├─ HubScan(InboundReceive) written (append-only)
   ├─ PieceStatus::ScannedIn
   ├─ HubInventory created at staging zone location
   └─ Kafka: hub.piece.scanned_inbound
      → order-intake consumer: ShipmentStatus::AtHub
        ↓
3. Pieces sorted by destination country/zone
   ├─ Assigned to open pallet for that destination
   ├─ HubScan(PalletAssign) written
   └─ HubInventory.location_id updated to pallet's rack/shelf position
        ↓
4. Pallet sealed (hub agent action or weight threshold trigger)
   ├─ PalletStatus::Sealed
   └─ Kafka: hub.pallet.sealed
        ↓
5. Consolidation plan run (existing bin-pack against FCL20/FCL40/truck spec)
        ↓
6. Pallets loaded into container
   ├─ HubScan(OutboundLoad) per pallet
   ├─ ContainerStatus::Loading
   └─ HubInventory updated to container
        ↓
7. Manifest finalised
   ├─ HubTransferManifest created (carrier_booking_ref = MAWB or B/L)
   └─ ContainerStatus::Manifested
        ↓
8. Container sealed → ContainerStatus::Sealed
        ↓
9. Container handed to carrier → Container::depart()
   ├─ ContainerStatus::InTransit
   └─ Kafka: hub.container.departed
      → order-intake consumer: ShipmentStatus::InTransit (all master AWBs in container)
```

### 4.3 In Transit — Customs Gate

```
[Sea/Air only]
Port/airport arrival → Container::arrive_at_port()
   ├─ ContainerStatus::ArrivedAtPort
   ├─ HubTransferManifest.customs_status = Pending
   ├─ Broker API (optional): push manifest XML/JSON, receive duty estimate
   │    └─ broker_payload + duties_total_cents stored on manifest
   └─ Kafka: hub.container.arrived_at_port
      → engagement consumer: notify customer "shipment at port"
        ↓
Hub agent reviews in ops portal Customs Queue
   → Container::enter_customs()
   ├─ ContainerStatus::Customs
   ├─ HubTransferManifest.customs_status = Hold
   └─ Kafka: hub.container.customs_hold
      → order-intake consumer: ShipmentStatus::CustomsHold (all AWBs)
        ↓
Hub agent approves clearance [mandatory human gate]
   → Container::clear_customs()
   ├─ ContainerStatus::Released
   ├─ HubTransferManifest.customs_status = Cleared
   ├─ HubTransferManifest.cleared_by + cleared_at recorded
   └─ Kafka: hub.container.customs_cleared
      → order-intake consumer: ShipmentStatus::InTransit
      → payments consumer: if duties_total_cents > 0 → create duties invoice

[Domestic road]
Container arrives at destination hub → Container::release_domestic()
   ├─ ContainerStatus::Released
   ├─ HubTransferManifest.customs_status = NotRequired
   └─ Kafka: hub.container.released_domestic
```

### 4.4 Destination Hub — Inbound & Last-Mile Handoff

```
1. Container received at destination hub (Released state)
   → Hub agent scans container barcode in Driver App Hub Mode or Ops Portal
        ↓
2. Pallets unloaded; each pallet scanned
   ├─ PalletStatus::Arrived
   └─ HubInventory updated to staging zone at destination hub
        ↓
3. Pallet broken (break-bulk) → PalletStatus::Broken
   └─ Each piece scanned → HubScan(ContainerDeconsolidate)
      └─ HubInventory: piece moves from pallet location to sorting zone bin
        ↓
4. Container::deconsolidate() called
   ├─ ContainerStatus::Deconsolidated
   ├─ Returns Vec<ChildAwb>
   └─ Kafka: hub.container.deconsolidated
      → order-intake consumer: ShipmentStatus::AtHub (destination)
        ↓
5. Per-piece: delivery address resolved to local last-mile zone
   ├─ HubScan(LocalSortAssign) written
   └─ HubInventory.location_id updated to assigned delivery cage/bin
        ↓
6. Pieces loaded to last-mile vehicle → HubScan(OutboundLoad)
   └─ PieceStatus::ScannedOut
        ↓
7. Routing rule evaluated per shipment (HubRoutingConfig lookup by destination_zone)
   ├─ OwnDriver  → Kafka: hub.shipment.dispatch_requested
   │               → dispatch consumer: auto-assign driver
   │               → ShipmentStatus::OutForDelivery
   ├─ Carrier    → Kafka: hub.shipment.carrier_booking_requested
   │               → carrier consumer: book 3PL via adapter
   │               → ShipmentStatus::OutForDelivery
   └─ Auto       → dispatch_requested first
                   → if no driver within auto_fallback_window_mins
                   → dispatch consumer publishes carrier_booking_requested
```

---

## 5. Kafka Event Inventory

| Topic | Emitted by | Primary consumers |
|---|---|---|
| `hub.piece.scanned_inbound` | hub-ops (InboundReceive) | order-intake → `AtHub` |
| `hub.pallet.sealed` | hub-ops | hub-ops internal |
| `hub.container.departed` | hub-ops | order-intake → `InTransit` |
| `hub.container.arrived_at_port` | hub-ops | engagement → customer notify |
| `hub.container.customs_hold` | hub-ops | order-intake → `CustomsHold` |
| `hub.container.customs_cleared` | hub-ops | order-intake → `InTransit`; payments → duties invoice |
| `hub.container.released_domestic` | hub-ops | — |
| `hub.container.deconsolidated` | hub-ops | order-intake → `AtHub` (dest) |
| `hub.shipment.dispatch_requested` | hub-ops | dispatch → assign driver |
| `hub.shipment.carrier_booking_requested` | hub-ops / dispatch (Auto fallback) | carrier → book 3PL |

All events carry: `tenant_id`, `event_id` (UUID), `occurred_at` (TIMESTAMPTZ), and the primary entity ID.

---

## 6. HTTP API

All under `/v1/hub-ops/`. Existing consolidation, pallet, and container routes are extended — not replaced.

### Locations
```
POST   /locations                         Create HubLocation (admin)
GET    /locations?hub_id=&zone_id=        List locations in hub/zone
GET    /locations/:id                     Location detail + current inventory
```

### Inventory
```
GET    /inventory?hub_id=&location_id=    List inventory at a location
POST   /inventory/move                    Move piece or pallet to new location
       { inventory_id, to_location_id, moved_by }
```

### Scans
```
POST   /scans                             Record a hub scan (primary scan endpoint)
       { hub_id, piece_awb, scan_type, pallet_id?, container_id?,
         device_timestamp, exception? }
GET    /scans?shipment_id=&hub_id=        Scan history for a shipment at a hub
```

### Container Operations (new endpoints)
```
POST   /containers/:id/arrive-at-port     InTransit → ArrivedAtPort (sea/air only)
POST   /containers/:id/enter-customs      ArrivedAtPort → Customs
POST   /containers/:id/clear-customs      Customs → Released
       { cleared_by, duties_total_cents?, customs_filing_ref? }
POST   /containers/:id/release-domestic   InTransit → Released (road mode)
POST   /containers/:id/deconsolidate      Released → Deconsolidated; triggers routing fan-out
```

### Routing Config
```
GET    /routing-config?hub_id=            List routing rules for a hub
PUT    /routing-config/:id                Update a rule (OwnDriver/Carrier/Auto)
```

### Transfer Manifest
```
GET    /manifests/:container_id           Get manifest for a container
PATCH  /manifests/:container_id           Update carrier_booking_ref or customs_filing_ref
```

---

## 7. MCP Tools

Per ADR-0004 — AI agent actions go through MCP only.

```
get_hub_inventory(hub_id, location_id?)   → current HubInventory snapshot
get_container_status(container_id)        → Container + HubTransferManifest
get_customs_queue(hub_id)                → containers in Customs state
assign_piece_to_pallet(piece_awb, pallet_id)
trigger_deconsolidation(container_id)    → calls deconsolidate() with agent audit trail
```

---

## 8. New Kafka Consumers

### order-intake — extend `status_consumer.rs`

| Event | ShipmentStatus |
|---|---|
| `hub.piece.scanned_inbound` | `AtHub` |
| `hub.container.departed` | `InTransit` |
| `hub.container.customs_hold` | `CustomsHold` |
| `hub.container.customs_cleared` | `InTransit` |
| `hub.container.deconsolidated` | `AtHub` |
| `hub.shipment.dispatch_requested` | `OutForDelivery` |
| `hub.shipment.carrier_booking_requested` | `OutForDelivery` |

### dispatch — new `hub_dispatch_consumer.rs`
- Consumes `hub.shipment.dispatch_requested`
- Calls existing auto-dispatch logic (same path as domestic last-mile)
- Auto fallback: if no driver available within `auto_fallback_window_mins` → publishes `hub.shipment.carrier_booking_requested`

### carrier — new `hub_carrier_consumer.rs`
- Consumes `hub.shipment.carrier_booking_requested`
- Selects carrier from `HubRoutingConfig.carrier_id`
- Books via existing carrier adapter (DHL, FedEx, Aramex, etc.)
- Creates `SlaRecord`

### payments — new `customs_duty_consumer.rs`
- Consumes `hub.container.customs_cleared`
- If `duties_total_cents > 0` → creates customs duties invoice
- Existing `BillingClearance` gate blocks container assignment until paid

### engagement — new `hub_milestone_consumer.rs`
- Consumes `hub.container.arrived_at_port`, `hub.container.customs_hold`, `hub.container.customs_cleared`
- Sends customer notifications per engagement engine templates

---

## 9. Frontend

### Driver App — Hub Mode

Activates for drivers with `role: hub_agent`. Replaces route task list with scan-first flows:

| Screen | Scan inputs | HubScan written |
|---|---|---|
| Inbound Receive | piece AWB | `InboundReceive` |
| Pallet Assign | piece AWB + pallet label | `PalletAssign` |
| Outbound Load | pallet label + container label | `OutboundLoad` |
| Break-Bulk | container label + each piece AWB | `ContainerDeconsolidate` |
| Local Sort | piece AWB + cage/bin location tag | `LocalSortAssign` |

All scans: `device_timestamp` = hardware clock at scan moment (not at upload time).

### Ops Portal — New Pages under `/hub-ops/`

| Page | Purpose |
|---|---|
| Container Board | Kanban by ContainerStatus; filter by mode, hub, date |
| Customs Queue | Containers in Customs state; broker pre-fill visible; "Approve Clearance" action |
| Inventory Map | Visual grid of HubLocation tree; click zone/aisle/shelf to see HubInventory |
| Routing Config | Table of HubRoutingConfig rules per hub+zone; editable |

---

## 10. Cross-Cutting Rules

1. **Dual timestamp on all hub scans:** Every `HubScan` carries `device_timestamp` (hardware clock) and `server_timestamp` (backend receipt). SLA calculations use `device_timestamp` where non-null.

2. **`hub_scans` is append-only:** No `UPDATE` or `DELETE`. Enforced at DB layer via REVOKE grant.

3. **Customs clearance is always a human gate:** The broker API pre-populates form data but the hub agent always clicks "Approve". `clear_customs()` requires `cleared_by: Uuid`.

4. **POP prerequisite (existing rule):** `ProofOfPickup.status = Completed` must exist before a piece can be scanned `InboundReceive` at origin hub. This ties into the existing billing clearance gate.

5. **`workflow_metadata` for exceptions:** Any scan exception (`Missing`, `Damaged`, `WeightMismatch`) is written to the associated invoice's `workflow_metadata` JSONB. Non-blocking — ops visibility tag only.

6. **`HubInventory` is the only mutable hub entity:** All others (`HubScan`, `shipment_telemetry_logs`) are append-only. `HubInventory.location_id` is updated in place when inventory physically moves.

7. **`Deconsolidated` is the terminal Container state.** `Delivered` is deprecated for new flows. Retained in the enum for backward compatibility with any existing data.

---

## 11. Database Migrations

| Service | Migration file | Creates |
|---|---|---|
| hub-ops | `0004_create_hub_scans.sql` | `hub_ops.hub_scans` (append-only) |
| hub-ops | `0005_create_hub_locations.sql` | `hub_ops.hub_locations` |
| hub-ops | `0006_create_hub_inventory.sql` | `hub_ops.hub_inventory` |
| hub-ops | `0007_create_hub_transfer_manifests.sql` | `hub_ops.hub_transfer_manifests` |
| hub-ops | `0008_create_hub_routing_configs.sql` | `hub_ops.hub_routing_configs` |
| libs/types | — | Add `Deconsolidated` to `ContainerStatus` enum |

All migrations run via `logisticos_common::migrations::run` per ADR-0012.

---

## 12. Glossary Additions

| Term | Definition |
|---|---|
| **Hub Mode** | Driver App role variant for hub agents; unlocks scan-first UI replacing route task list |
| **Break-bulk** | Disassembling a pallet at destination hub into individual pieces for last-mile sorting |
| **Deconsolidation** | The act of breaking a container back into individual ChildAWBs for local dispatch; terminal Container event |
| **HubLocation** | Structural tree entity representing a physical slot inside a hub warehouse (zone → aisle → rack → shelf → bin) |
| **HubInventory** | Mutable ledger tracking current `HubLocation` of a piece or pallet |
| **HubTransferManifest** | Customs/freight document wrapping a container; holds MAWB/B/L, broker ref, duties, clearance audit |
| **RoutingType** | OwnDriver | Carrier | Auto — decides last-mile handoff strategy at container release |
| **CustomsStatus** | NotRequired | Pending | Hold | Cleared — state of a manifest's customs clearance |
| **Auto fallback window** | Minutes after dispatch_requested before the system escalates to carrier booking (Auto routing mode) |

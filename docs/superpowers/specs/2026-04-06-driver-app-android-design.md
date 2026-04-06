# Driver Super App — Native Android Kotlin
**Date:** 2026-04-06
**Status:** Approved
**Location:** `apps/driver-app-android/`

---

## Overview

A full production native Android driver super app for LogisticOS, built in Kotlin from scratch. Replaces the incomplete React Native scaffold at `apps/driver-app/`. Drivers use this app to manage their full shift — from login through route navigation, package scanning, proof of delivery, and offline operation.

---

## 1. Tech Stack

| Layer | Technology |
|---|---|
| Language | Kotlin 2.0 |
| UI | Jetpack Compose + Material 3 |
| Architecture | MVVM + Clean Architecture (data / domain / presentation) |
| DI | Hilt |
| Navigation | Compose Navigation |
| Async | Kotlin Coroutines + Flow |
| Networking | Retrofit + OkHttp + kotlinx.serialization |
| Local DB | Room (SQLite) |
| Background work | WorkManager |
| Location | Fused Location Provider via Android ForegroundService |
| Maps rendering | Mapbox Maps SDK for Android (dark style) |
| Routing | Google Maps Directions API (REST) |
| Barcode scanning | Google ML Kit + Zebra/Honeywell intent fallback |
| Image capture | CameraX |
| Signature capture | Custom Compose Canvas component |
| Auth | JWT via Identity Service, stored in EncryptedSharedPreferences |
| Push notifications | Firebase Cloud Messaging (FCM) |
| Root detection | RootBeer library |
| Min SDK | API 26 (Android 8.0) |
| Target SDK | API 35 (Android 15) |

---

## 2. Module Structure

```
driver-app-android/
├── app/                          # Application module, Hilt setup, NavGraph
├── core/
│   ├── network/                  # Retrofit, OkHttp interceptors, JWT refresh
│   ├── database/                 # Room DB, DAOs, entities
│   ├── location/                 # ForegroundService, FusedLocationProvider
│   └── common/                   # Extensions, utils, constants, BuildConfig
├── feature/
│   ├── auth/                     # Login, OTP, biometric
│   ├── home/                     # Shift dashboard, stats
│   ├── route/                    # Stop list, reorder, re-optimization
│   ├── navigation/               # Mapbox map, turn-by-turn, Google routing
│   ├── delivery/                 # Delivery flow, state machine, status updates
│   ├── pod/                      # Photo, signature, OTP capture
│   ├── scanner/                  # ML Kit + hardware scanner
│   ├── pickup/                   # Pickup flow at merchant/hub
│   ├── notifications/            # FCM, in-app alerts, notification list
│   └── profile/                  # Driver profile, vehicle, app settings
└── buildSrc/                     # Version catalog (libs.versions.toml)
```

---

## 3. Authentication & Session Management

### Flow
```
Driver enters phone number
        ↓
Identity Service sends SMS OTP
        ↓
Driver enters OTP → Identity Service returns JWT + Refresh Token
        ↓
JWT stored in EncryptedSharedPreferences (AES-256)
        ↓
All API calls attach JWT as Bearer token via OkHttp AuthInterceptor
        ↓
On 401 → TokenAuthenticator auto-refreshes JWT, issues new Refresh Token
        ↓
On refresh failure → force logout, navigate to LoginScreen
```

### Token Configuration
- **JWT TTL:** 30 minutes
- **Refresh Token TTL:** 30 days
- **Token Rotation:** Every JWT refresh issues a new Refresh Token; old token invalidated immediately (prevents reuse attacks)
- **Storage:** Both tokens in EncryptedSharedPreferences (AES-256); never in plain SharedPreferences or local DB

### Biometric Unlock
- After first login, subsequent app opens use fingerprint/face via Android BiometricPrompt
- No re-OTP required unless Refresh Token expires
- Biometric key stored in Android Keystore

### Offline Auth Behaviour
- JWT valid + offline → proceed normally
- JWT expired + offline → **Offline Mode Active** state:
  - POD capture allowed, queued for sync
  - Delivery completion allowed, queued for sync
  - Profile changes and sensitive settings blocked
  - Amber banner shown: "Offline Mode Active — reconnect to sync"
- Refresh Token expired + offline → "Reconnect to continue" screen (cannot work)

### Screens
- `PhoneScreen` — phone number entry with country picker
- `OtpScreen` — 6-digit OTP, 60s resend timer
- `BiometricScreen` — system BiometricPrompt on subsequent launches

---

## 4. Offline-First Data Architecture

Room is the **source of truth**. All UI reads from Room. Network is a sync layer only — no UI reads directly from network responses.

### Room Entities

| Entity | Purpose |
|---|---|
| `ShiftEntity` | Current shift, assigned stops, start/end time |
| `TaskEntity` | Each delivery/pickup stop — address, recipient, status, POD requirements |
| `RouteEntity` | Ordered stop sequence, polyline, ETA per stop |
| `PodEntity` | Photo path, signature path, OTP token, sync status |
| `LocationBreadcrumbEntity` | GPS points queued for upload (lat, lng, timestamp, accuracy) |
| `ScanEventEntity` | Barcode scans tied to a task — AWB, timestamp, sync status |
| `SyncQueueEntity` | Generic outbound queue — action type, payload JSON, retry count, last error |

### Sync Strategy

**Outbound (driver → server):**
- POD submissions, status updates, scan events, breadcrumb batches
- WorkManager periodic task every 60 seconds when online
- Exponential backoff on failure: 1s → 2s → 4s → max 5 min

**Inbound (server → driver):**
- Full pull on shift start
- FCM push triggers incremental pull mid-shift
- WorkManager periodic pull every 5 minutes as fallback

**Conflict resolution:**
- Server wins for task assignments and route changes
- Device wins for POD data (driver's capture is authoritative)

### Offline Capability Matrix

| Action | Offline Allowed |
|---|---|
| View task list & route | Yes — Room cache |
| Navigate to stop | Yes — cached route + Mapbox offline tiles |
| Capture POD (photo/sig/OTP) | Yes — queued in SyncQueue |
| Mark delivery complete | Yes — local status update, queued |
| Barcode scan | Yes — ScanEvent queued |
| Receive new tasks | No — requires server |
| Profile changes | No — blocked in Offline Mode Active |

### Mapbox Offline Tiles
At shift start (online), the app pre-downloads Mapbox offline tile packs for the bounding box covering all shift stops. Navigation works fully offline.

---

## 5. Location Tracking & Navigation

### Foreground Service
A persistent `LocationForegroundService` runs for the entire shift duration. Persistent notification: "LogisticOS — Shift Active".

**Adaptive frequency:**
| Condition | Update interval |
|---|---|
| Speed > 5 km/h (driving) | Every 2 seconds |
| Speed 0–5 km/h (slow/stopped) | Every 15 seconds |
| Stationary > 2 minutes | Every 30 seconds |

GPS points written to `LocationBreadcrumbEntity` → WorkManager uploads batched points every 30 seconds.

### Navigation Flow
```
Driver taps "Navigate" on a stop
        ↓
Google Directions API (REST): origin=GPS, destination=address, mode=driving
        ↓
Route polyline + steps stored in RouteEntity
        ↓
Mapbox renders:
  - Dark map (Mapbox Streets Dark style)
  - Neon cyan route polyline
  - Animated driver marker (arrow following heading)
  - Stop markers: purple=pending, green=completed, amber=attempted
        ↓
Turn-by-turn banner at top — next maneuver + distance + street name
        ↓
On arrival (within 50m of stop) → auto-prompt delivery flow
```

### Re-Optimization Triggers
A re-optimization request is sent to the Dispatch service when:
- Driver marks a stop as Failed / Attempted
- Dispatcher adds a new stop mid-shift (FCM push)
- Driver manually reorders stops
- ETA deviation > 20 minutes from original plan

---

## 6. Delivery & POD Flow

### Task State Machine
```
ASSIGNED → EN_ROUTE → ARRIVED → IN_PROGRESS → COMPLETED
                                      ↓
                                ATTEMPTED (no one home / access denied)
                                      ↓
                                FAILED (refused / wrong address / damaged)
                                      ↓
                                RETURNED (undelivered at end of shift)
```

### Arrival Flow
```
Driver within 50m → auto-trigger arrival
        ↓
ArrivalScreen: recipient name + phone, instructions, POD requirements badge, package list
        ↓
Driver taps "Start Delivery"
        ↓
Package scan (if required) → all AWBs must be scanned before proceeding
        ↓
POD capture (per shipment configuration):
  Photo → Signature → OTP  (order fixed, all required steps must complete)
        ↓
CompleteScreen → auto-advance to next stop
```

### POD Capture Modes
| Mode | Implementation |
|---|---|
| **Photo** | CameraX viewfinder → capture → preview → confirm or retake |
| **Signature** | Full-screen Compose Canvas → recipient draws → confirm or clear |
| **OTP** | Driver taps "Send OTP" → Engagement service SMS recipient → driver enters 6 digits → server validates |

All three configurable per shipment by merchant at booking time.

### Failed Delivery Flow
```
Driver taps "Cannot Deliver"
        ↓
Reason picker: No one home / Refused / Wrong address / Access denied / Damaged
        ↓
Photo of premises required (evidence)
        ↓
Task → ATTEMPTED, attempt count incremented
Re-delivery options shown if merchant configured
```

### COD Handling
COD amount shown to driver before POD capture. Driver confirms collection after receipt. COD reconciliation synced to Payments service.

---

## 7. Barcode Scanner

### Unified Interface
```kotlin
interface ScannerManager {
    fun startScan(onResult: (ScanResult) -> Unit)
    fun stopScan()
    val isHardwareScanner: Boolean
}
```
Hilt provides correct implementation at runtime — screens are scanner-agnostic.

### ML Kit Path (standard Android phones)
- CameraX preview with real-time ML Kit BarcodeScanner
- On detection: haptic feedback + beep + green overlay box
- Supports QR, Code 128, Data Matrix, and all major 1D/2D formats

### Hardware Scanner Path (Zebra / Honeywell)
- `BroadcastReceiver` registered for:
  - Zebra: `com.symbol.datawedge.api.RESULT_ACTION`
  - Honeywell: `com.honeywell.aidc.action.ACTION_AIDC_DATA`
- Same validation + feedback logic as ML Kit path

### Scan Validation
| Result | Behaviour |
|---|---|
| AWB matches expected package | Green checkmark, proceed |
| AWB not in expected list | Amber warning "Unexpected package — confirm?" |
| Duplicate scan | "Already scanned" toast |

### Batch Scan Mode (hub pickups)
Running tally: `[12 / 15 scanned]`. All warnings must be resolved before proceeding. Unscanned packages prompt explicit confirmation.

---

## 8. Screens & Navigation Structure

### Navigation Graph
```
AppNavGraph
├── AuthGraph
│   ├── PhoneScreen
│   ├── OtpScreen
│   └── BiometricScreen
│
└── ShiftGraph
    ├── HomeScreen
    ├── RouteScreen
    ├── NavigationScreen
    ├── DeliveryGraph
    │   ├── ArrivalScreen
    │   ├── ScannerScreen
    │   ├── PodScreen
    │   ├── CompleteScreen
    │   └── FailedScreen
    ├── PickupGraph
    │   ├── PickupListScreen
    │   ├── ScannerScreen
    │   └── PickupConfirmScreen
    ├── NotificationsScreen
    └── ProfileScreen
```

### Bottom Navigation (ShiftGraph)
| Tab | Screen |
|---|---|
| Home | HomeScreen — shift status, stats, active stop |
| Route | RouteScreen — ordered stop list, drag-to-reorder |
| Scan | ScannerScreen — quick-launch barcode scanner |
| Notifications | NotificationsScreen — FCM alerts |
| Profile | ProfileScreen — driver info, settings |

### Key Screen Details

**HomeScreen:** Shift status card, today's stats (stops assigned/completed/failed/COD collected), active stop card with ETA, Start/End Shift CTA, Offline Mode Active banner.

**RouteScreen:** Ordered stop list with status chips, drag-to-reorder handles, ETA per stop + total shift ETA, Re-optimize button, completed stops collapsed at bottom.

**NavigationScreen:** Full-screen Mapbox dark map, driver marker + neon cyan polyline, next turn instruction banner (top), stop info bottom sheet (address, recipient, distance), Arrived button (also auto-triggers on geofence).

**PodScreen:** Tabbed by POD requirement (Photo / Signature / OTP), progress indicator, Submit enabled only when all required tabs complete.

---

## 9. Push Notifications & Real-Time Updates

### Architecture
```
Dispatch service → Kafka event → Engagement Engine → FCM → driver device
        ↓
FirebaseMessagingService receives
        ↓
Foreground: in-app banner (slides down, 4s auto-dismiss, tappable)
Background: system notification tray
```

### Notification Types
| Type | Priority | Tap Action |
|---|---|---|
| New stop assigned | High | RouteScreen, highlight new stop |
| Stop cancelled | High | RouteScreen, stop removed |
| Route re-optimized | High | RouteScreen, new order |
| Dispatch message | Normal | NotificationsScreen |
| COD amount updated | Normal | Active DeliveryScreen |
| Shift reminder | Low | HomeScreen |
| System alert | Low | NotificationsScreen |

### Token Management
FCM token registered with Identity Service on login. Token refresh handled by `onNewToken()`. Token scoped to `driver_id + tenant_id`.

### Offline Behaviour
FCM queues notifications for up to 4 weeks. On reconnect, incremental sync triggered immediately.

---

## 10. API Integration & Network Layer

### OkHttp Interceptor Chain
```
AuthInterceptor       → attaches JWT Bearer token
TenantInterceptor     → attaches X-Tenant-ID header
LoggingInterceptor    → debug builds only
        ↓
TokenAuthenticator    → on 401: refresh JWT + new Refresh Token, retry once
                      → on refresh failure: force logout
```

### Retrofit Service Interfaces
| Interface | Backend Service | Key Endpoints |
|---|---|---|
| `IdentityApiService` | Identity & Auth | `/auth/otp/send`, `/auth/otp/verify`, `/auth/refresh` |
| `DriverOpsApiService` | Driver Operations | `/shifts`, `/tasks`, `/tasks/{id}/status` |
| `DispatchApiService` | Dispatch & Routing | `/routes/optimize`, `/routes/{id}` |
| `PodApiService` | Proof of Delivery | `/pod/submit` (multipart) |
| `TrackingApiService` | Fleet / Tracking | `/location/batch` |
| `PaymentsApiService` | Payments | `/cod/confirm`, `/cod/reconcile` |

### Repository Pattern
All UI reads from Room via Repository. Network calls update Room; UI reacts via Flow. UI never touches network directly.

### POD Upload
Multipart body: `task_id` + `photo.jpg` (JPEG, max 1MB) + `sig.png` (PNG) + `otp_token`.

### Environment Configuration
Base URLs in `BuildConfig` via `productFlavors` — `dev`, `staging`, `prod`. No hardcoded URLs anywhere.

---

## 11. Security & Build Configuration

### Sensitive Data Storage
| Data | Storage |
|---|---|
| JWT | EncryptedSharedPreferences (AES-256) |
| Refresh Token | EncryptedSharedPreferences (AES-256) |
| Biometric key | Android Keystore |
| POD photos / signatures | Internal app storage only |
| Mapbox access token | `local.properties` → `BuildConfig` |
| Google Maps API key | `local.properties` → manifest placeholder |

### Certificate Pinning
OkHttp `CertificatePinner` pins Identity Service and Driver Ops Service TLS certs. Updated via app release only.

### Build Variants
```
productFlavors: dev | staging | prod
buildTypes:     debug | release (R8 full obfuscation on release)
```

### Android Permissions
```
INTERNET, ACCESS_NETWORK_STATE
ACCESS_FINE_LOCATION, ACCESS_COARSE_LOCATION, ACCESS_BACKGROUND_LOCATION
FOREGROUND_SERVICE, FOREGROUND_SERVICE_LOCATION
CAMERA, VIBRATE, RECEIVE_BOOT_COMPLETED
POST_NOTIFICATIONS, USE_BIOMETRIC
```

### Root Detection
RootBeer check on app start. Rooted devices: warning banner shown, event logged to server. POD from rooted devices flagged in backend. Does not block operation.

---

## 12. Testing Strategy

### Unit Tests
- All ViewModels via `kotlinx-coroutines-test` + Turbine
- All Repositories with MockK mocks
- Domain logic: state machine transitions, sync queue ordering, adaptive location frequency, token rotation
- **Target: 80% coverage on domain + ViewModel layer**

### Integration Tests
- Room DAOs with in-memory database
- WorkManager tasks via `TestWorkerBuilder`
- `ScannerManager` — mock ML Kit and hardware intent paths
- **Target: all key data flows covered**

### UI / E2E Tests (Compose + Espresso)
- Full delivery flow: arrive → scan → photo → signature → OTP → complete
- Auth flow: phone → OTP → biometric
- Offline mode: disable network → complete POD → re-enable → assert sync
- Navigation: all bottom nav tabs load correct screens
- **Target: all critical user journeys covered**

### Test Tools
| Tool | Purpose |
|---|---|
| JUnit 5 | Unit test runner |
| MockK | Kotlin mocking |
| Turbine | Flow / StateFlow assertions |
| Hilt Testing | DI in instrumented tests |
| Compose Testing | UI interaction + assertions |
| OkHttp MockWebServer | Fake API responses |
| Robolectric | Unit tests needing Android context |

### CI Gates (GitHub Actions)
- Unit tests on every PR
- Integration tests on merge to `main`
- UI tests on release build before Play Store submission

---

## Non-Functional Requirements

- App cold start < 2 seconds
- Location update latency < 2 seconds end-to-end while driving
- Barcode scan recognition < 500ms (ML Kit)
- POD photo upload < 10 seconds on 3G
- Room query response < 50ms for task list
- Offline tile pre-download completes before driver departs hub

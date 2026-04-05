# LogisticOS Customer App — Complete Design

> **Scope:** Customer app for booking shipments, tracking deliveries, managing history, contacting support, and managing profile. 4-phase implementation: core screens → API integration → animations → offline capability.

---

## Phase 1: Core 5 Screens (MVP UI)

### 1.1 Home Screen (Dashboard)

**Purpose:** Quick access to key actions and status at a glance.

**Layout:**
- Hero header: Greeting + loyalty points balance
- 4 quick-action cards: Book New, Track Shipment, View History, Contact Support
- Recent activity: 3 most recent shipments with status badges
- Promotional banner: Next shipment discount or loyalty offer
- Settings link (bottom)

**Components:**
- `HomeScreen.tsx` — Main screen
- `RecentShipmentCard.tsx` — Reusable card showing shipment summary
- `QuickActionButton.tsx` — 4-action grid
- `LoyaltyBanner.tsx` — Points display with promo

**State Dependencies:**
- Auth: customer name, loyalty points
- Shipments: list of recent shipments (Redux)
- Tracking: recent tracking history

**Interactions:**
- Tap quick-action card → navigate to corresponding screen
- Tap recent shipment → open tracking detail (modal or nav)
- Tap loyalty banner → open loyalty program screen (bonus feature)

---

### 1.2 Booking Screen

**Purpose:** Allow customers to book new shipments (local or international).

**Flow:**
1. Shipment Type Selection: Local vs International (toggle)
2. Pickup & Delivery Details
   - Pickup location (autocomplete from saved addresses or map)
   - Recipient name, phone, address (with country select for international)
3. Package Details
   - Description (text input)
   - Weight/dimensions (with preset sizes: small, medium, large)
   - Cargo type selector (documents, goods, fragile, etc.)
   - COD option (toggle) + amount if enabled
4. Service Selection
   - Delivery speed (standard, express, next-day)
   - For international: freight mode (sea, air) + estimated delivery
5. Review & Confirm
   - Summary of shipment details
   - Calculated fee breakdown
   - Commission slider (for merchants) or fixed rate display
   - Submit button → creates shipment → shows confirmation + AWB

**Components:**
- `BookingScreen.tsx` — Main orchestrator
- `ShipmentTypeToggle.tsx` — Local vs International
- `AddressInput.tsx` — Auto-complete + map picker (uses Expo Location)
- `PackageDetailsForm.tsx` — Weight, dimensions, cargo type
- `ServiceSelector.tsx` — Delivery speed, freight mode for international
- `FeeBreakdown.tsx` — Cost calculator
- `BookingConfirmation.tsx` — Success screen with AWB and tracking link

**State Dependencies:**
- Auth: customer phone, recent addresses
- Shipments: Redux slice for creating new shipment

**API Calls:**
- POST `/v1/shipments` (order-intake:8004) — Create shipment
- GET `/v1/addresses/autocomplete?q=...` — Address autocomplete (optional, can use static list)

**Validation:**
- Phone number format (E.164)
- Weight within limits (max 50kg for standard, 100kg for air/sea)
- Recipient address required for domestic + country for international
- COD amount required if COD enabled

---

### 1.3 History Screen

**Purpose:** View all past shipments with filtering and search.

**Layout:**
- Search bar (by AWB or date range)
- Filter chips: All / Delivered / Failed / In Transit / Cancelled
- List of shipments (pagination: 20 per page, lazy load on scroll)
- Each list item shows: AWB, status badge, origin → destination, date, fee

**Interactions:**
- Tap shipment → open tracking detail (same modal as from Home)
- Tap filter chip → filter list in place
- Type in search → filter by AWB or date
- Scroll to bottom → load next 20 shipments

**Components:**
- `HistoryScreen.tsx` — Main screen
- `ShipmentListItem.tsx` — Card with status, route, date
- `FilterChip.tsx` — Reusable chip component
- `ShipmentDetailModal.tsx` — Shows full tracking (reused from tracking screen)

**State Dependencies:**
- Shipments: paginated list from Redux
- Auth: customer ID for API calls

**API Calls:**
- GET `/v1/shipments?customer_id=X&status=&skip=0&limit=20` (order-intake:8004)

---

### 1.4 Support Screen

**Purpose:** Contact customer support via chat, FAQ, or ticket system.

**Layout:**
- FAQ section (collapsible): Common questions grouped by category
- Live chat button (prominent) → opens chat interface (or external link to support portal)
- Quick help buttons: Report Issue, Reschedule Delivery, Request Return
- Contact info: Email + phone (copyable)

**Components:**
- `SupportScreen.tsx` — Main screen
- `FAQSection.tsx` — Expandable FAQ categories
- `LiveChatButton.tsx` — Launch chat or link
- `QuickHelpAction.tsx` — Pre-filled issue templates

**State Dependencies:**
- Auth: customer email, phone (for support context)

**Interactions:**
- Tap FAQ → expand/collapse answer
- Tap "Report Issue" → pre-fill form with recent shipments (multi-select)
- Tap "Live Chat" → open chat interface (can be external URL or in-app)

**Note:** Full live chat implementation deferred to Phase 2+. For now, use static FAQ + links to external support portal.

---

### 1.5 Profile Screen

**Purpose:** Manage customer account, addresses, preferences, and logout.

**Sections:**
1. Account Info (read-only with edit button)
   - Name, phone, email, verification tier
   - KYC status badge
2. Saved Addresses
   - List of saved pickup/delivery addresses
   - Add new address button
   - Edit / Delete per address
3. Preferences
   - Notification toggles (delivery updates, promotions)
   - Language preference (EN/PH)
   - Currency (PHP/USD for international)
4. Help & Legal
   - Terms of Service link
   - Privacy Policy link
   - Contact Support link
5. Logout Button

**Components:**
- `ProfileScreen.tsx` — Main screen
- `AccountInfoCard.tsx` — Read-only account details with edit link
- `SavedAddressList.tsx` — Address management
- `AddressFormModal.tsx` — Add/edit address
- `PreferencesSection.tsx` — Toggle controls
- `LegalLinksSection.tsx` — Footer links

**State Dependencies:**
- Auth: name, phone, email, KYC status, loyalty points
- Prefs: notification settings, language, currency
- (Future) Addresses slice in Redux

**API Calls:**
- GET `/v1/customers/:id` (identity:8001) — Fetch full profile
- PUT `/v1/customers/:id` (identity:8001) — Update profile
- POST `/v1/addresses` (order-intake:8004) — Create address
- PUT `/v1/addresses/:id` (order-intake:8004) — Update address
- DELETE `/v1/addresses/:id` (order-intake:8004) — Delete address

---

## Phase 2: Backend Integration

### 2.1 API Client & Authentication

**Files:**
- `src/services/api/client.ts` — Axios instance with interceptors
- `src/services/api/auth.ts` — Login/verify phone, refresh token
- `src/services/api/shipments.ts` — Create, fetch, update shipments
- `src/services/api/tracking.ts` — Fetch tracking data
- `src/services/api/customers.ts` — Fetch/update customer profile

**Key Requirements:**
- Interceptors for JWT token injection in headers
- Automatic token refresh on 401 responses
- Error handling with retry logic (exponential backoff)
- Type-safe request/response with TypeScript
- Base URL configurable via ENV (development vs production)

**Authentication Flow:**
1. Phone → Verify OTP (identity:8001)
2. OTP → Receive JWT token (identity:8001)
3. Every request includes JWT in `Authorization: Bearer <token>` header
4. Token stored in secure storage (`expo-secure-store`)
5. On 401 → use refresh token to get new JWT
6. Refresh token stored securely as well

---

### 2.2 Real API Endpoints Integration

**Order Intake Service (8004):**
- POST `/v1/shipments` — Create shipment
- GET `/v1/shipments/:awb` — Fetch shipment by AWB
- GET `/v1/shipments?customer_id=X&status=Y&skip=Z&limit=L` — List shipments with filters
- PUT `/v1/shipments/:awb` — Update shipment (reschedule, cancel)
- GET `/v1/addresses/autocomplete?q=query` — Address suggestions (optional)

**Tracking Service (Embedded in Order Intake or Separate):**
- GET `/v1/tracking/:awb` — Fetch live tracking data
- Websocket `/ws/tracking/:awb` — Live tracking updates (Phase 3+, optional)

**Identity Service (8001):**
- POST `/v1/auth/verify-phone` — Send OTP
- POST `/v1/auth/verify-otp` — Verify OTP, return JWT
- GET `/v1/customers/:id` — Fetch customer profile
- PUT `/v1/customers/:id` — Update profile

**Error Handling:**
- Network errors → Show toast + retry button
- 4xx errors → Show user-friendly error message
- 5xx errors → Show "Service unavailable" + retry
- Timeout → Retry with exponential backoff (max 3 retries)

---

## Phase 3: UX & Animations

### 3.1 Micro-interactions

Every screen interaction should animate smoothly using React Native Reanimated:
- **Navigation transitions:** Slide from right (default), fade for modals
- **List item entry:** Fade + translate-up with stagger (100ms delay between items)
- **Button press:** Haptic feedback + scale animation (0.95 on press, back to 1.0 on release)
- **Loading state:** Skeleton loaders (pulsing) for list items
- **Error state:** Shake animation for input fields with validation errors
- **Success state:** Checkmark animation with confetti (optional, for booking confirmation)

### 3.2 Status Badge Animations

Status badges should pulse gently with neon glow when actively being tracked:
- Out for delivery → Cyan pulse
- Delivered → Green pulse (brief, then static)
- Failed → Red pulse
- In transit → Purple pulse

### 3.3 Gesture Interactions

- **Swipe to dismiss:** Modal cards (pan gesture → dismiss on threshold)
- **Swipe to refresh:** List screens (pull-down, spring-back animation)
- **Tap to expand:** FAQ items, address cards

---

## Phase 4: Offline Capability

### 4.1 Local Database (SQLite)

**Tables:**
- `shipments` — Local cache of customer's shipments
- `tracking_history` — Last-known tracking state for each AWB
- `saved_addresses` — Offline address book
- `synced_metadata` — Timestamps of last sync per resource

**Strategy:**
- On successful API call → Write to local DB
- On network unavailable → Read from local DB, show "Offline" indicator
- On network restored → Sync local changes back to server (conflict resolution: server wins)

### 4.2 Offline Booking

**Flow:**
1. Customer fills booking form (offline or online)
2. On submit:
   - If online → POST to `/v1/shipments` immediately
   - If offline → Save to local `pending_shipments` table, show "Will sync when online"
3. When online restored → Sync pending shipments, handle duplicates

**Implementation:**
- Background sync using `expo-task-manager` and `expo-background-fetch`
- Sync runs periodically (every 15 min) or on network state change
- Toast notification when sync succeeds/fails

### 4.3 Offline Tracking

- Display cached tracking data when offline
- Show "Last updated X minutes ago" badge
- Refresh button available (only works if online)
- Queue tracking refreshes on network restored

---

## Data Flow & Redux Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Redux Store                             │
├─────────────────────────────────────────────────────────────┤
│ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐   │
│ │   auth   │ │shipments │ │ tracking │ │   prefs      │   │
│ │          │ │          │ │          │ │              │   │
│ │ token    │ │  list[]  │ │ history[]│ │notifEnabled  │   │
│ │ profile  │ │ byAwb{}  │ │ byAwb{}  │ │ language     │   │
│ │ isGuest  │ │          │ │          │ │ currency     │   │
│ └──────────┘ └──────────┘ └──────────┘ └──────────────┘   │
└─────────────────────────────────────────────────────────────┘
         ↓                      ↓
    ┌─────────────────────────────────────┐
    │   API Service Layer (src/services)  │
    │  - client.ts (axios + interceptors) │
    │  - auth.ts (login/otp)              │
    │  - shipments.ts (CRUD)              │
    │  - tracking.ts (fetch tracking)     │
    │  - customers.ts (profile)           │
    └─────────────────────────────────────┘
         ↓                      ↓
    ┌─────────────────────────────────────┐
    │   Offline Layer (SQLite + Cache)    │
    │  - Local DB sync on success         │
    │  - Fallback to cache on error       │
    │  - Background sync on reconnect     │
    └─────────────────────────────────────┘
```

---

## File Structure (Phase 1-4)

```
apps/customer-app/
├── src/
│   ├── screens/
│   │   ├── home/
│   │   │   ├── HomeScreen.tsx
│   │   │   ├── RecentShipmentCard.tsx
│   │   │   ├── QuickActionButton.tsx
│   │   │   └── LoyaltyBanner.tsx
│   │   ├── booking/
│   │   │   ├── BookingScreen.tsx
│   │   │   ├── ShipmentTypeToggle.tsx
│   │   │   ├── AddressInput.tsx
│   │   │   ├── PackageDetailsForm.tsx
│   │   │   ├── ServiceSelector.tsx
│   │   │   ├── FeeBreakdown.tsx
│   │   │   └── BookingConfirmation.tsx
│   │   ├── history/
│   │   │   ├── HistoryScreen.tsx
│   │   │   ├── ShipmentListItem.tsx
│   │   │   ├── FilterChip.tsx
│   │   │   └── ShipmentDetailModal.tsx
│   │   ├── support/
│   │   │   ├── SupportScreen.tsx
│   │   │   ├── FAQSection.tsx
│   │   │   ├── LiveChatButton.tsx
│   │   │   └── QuickHelpAction.tsx
│   │   ├── profile/
│   │   │   ├── ProfileScreen.tsx
│   │   │   ├── AccountInfoCard.tsx
│   │   │   ├── SavedAddressList.tsx
│   │   │   ├── AddressFormModal.tsx
│   │   │   ├── PreferencesSection.tsx
│   │   │   └── LegalLinksSection.tsx
│   │   └── tracking/
│   │       ├── TrackingScreen.tsx (existing, enhance)
│   │       └── TrackingDetailModal.tsx (extracted from inline)
│   ├── services/
│   │   └── api/
│   │       ├── client.ts (update with real config)
│   │       ├── auth.ts
│   │       ├── shipments.ts
│   │       ├── tracking.ts
│   │       └── customers.ts
│   ├── store/
│   │   ├── index.ts (update with new slices)
│   │   ├── slices/
│   │   │   ├── auth.ts (extracted)
│   │   │   ├── shipments.ts (extracted)
│   │   │   ├── tracking.ts (extracted)
│   │   │   ├── prefs.ts (extracted)
│   │   │   └── addresses.ts (new)
│   │   └── hooks.ts (useAppDispatch, useAppSelector)
│   ├── db/
│   │   ├── sqlite.ts (Phase 4: SQLite connection)
│   │   ├── schema.ts (Phase 4: table definitions)
│   │   └── sync.ts (Phase 4: offline sync logic)
│   ├── hooks/
│   │   ├── useApi.ts (Phase 2: API call wrapper with offline fallback)
│   │   ├── useTracking.ts (Phase 2: fetch + auto-refresh tracking)
│   │   ├── useShipments.ts (Phase 2: fetch + cache shipments)
│   │   └── useAnimation.ts (Phase 3: reusable animation values)
│   ├── components/
│   │   ├── StatusBadge.tsx (reused across screens)
│   │   ├── ShipmentCard.tsx (reused)
│   │   ├── Button.tsx (theme-aware, animated)
│   │   ├── Input.tsx (with validation feedback)
│   │   ├── Modal.tsx (with animation)
│   │   ├── Toast.tsx (notifications)
│   │   └── SkeletonLoader.tsx (Phase 3)
│   ├── utils/
│   │   ├── formatting.ts (date, phone, currency)
│   │   ├── validation.ts (phone, email, address)
│   │   ├── navigation.ts (route helpers)
│   │   └── colors.ts (design tokens)
│   ├── navigation/ (existing, enhance)
│   │   └── AppNavigator.tsx
│   ├── App.tsx (existing)
│   └── store.ts (Redux setup, existing)
├── package.json
├── tsconfig.json
├── app.json (Expo config)
└── eas.json (EAS build config, for Phase 4 testing)
```

---

## Testing Strategy

**Unit Tests (Jest):**
- API client interceptors
- Redux reducers
- Validation utils
- Date/formatting utils

**Integration Tests (React Native Testing Library):**
- Screen navigation
- Form submission
- API call mocking (MSW or jest.mock)
- List filtering & pagination

**E2E Tests (Detox, optional Phase 3+):**
- Full user journey: signup → book → track → history
- Offline scenario: book offline → sync on reconnect

---

## Success Criteria

**Phase 1:** All 5 screens render correctly with mock data, navigation works, Redux state updates properly

**Phase 2:** API calls work against real backend, JWT auth works, shipments can be created and tracked

**Phase 3:** Animations are smooth (60fps), micro-interactions feel polished, loading states are clear

**Phase 4:** App works offline, pending shipments sync when online, cached tracking loads instantly

---

## Known Constraints & Decisions

1. **No live chat initially** — Support screen shows FAQ + static links. Live chat implementation deferred to Phase 2+.
2. **Address autocomplete optional** — Can start with manual input, add autocomplete in Phase 2 if time permits.
3. **Websocket tracking optional** — Phase 1-2 use polling. Websocket for live updates in Phase 3+.
4. **No push notifications initially** — Notifications handled by native OS. In-app toast for key events in Phase 1.
5. **Offline conflicts resolved by server** — Server-win strategy for simplicity. Implement client-side conflict UI in Phase 4 if needed.

---


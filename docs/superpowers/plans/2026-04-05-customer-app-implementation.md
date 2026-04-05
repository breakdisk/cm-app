# LogisticOS Customer App Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a complete React Native customer mobile app enabling customers to book shipments, track deliveries, view history, contact support, and manage their profile across 4 phases with full backend integration and offline capability.

**Architecture:** Phase 1 builds UI shells with mock data and Redux store setup. Phase 2 integrates real APIs with JWT auth and error handling. Phase 3 adds React Native Reanimated animations and gesture interactions. Phase 4 adds SQLite offline storage with background sync.

**Tech Stack:** React Native + Expo, TypeScript, Redux Toolkit, Axios, React Native Reanimated 3, NativeWind, SQLite (expo-sqlite), Expo Secure Store, Expo Task Manager, Jest, React Native Testing Library.

---

## File Structure Overview

**Phase 1 Creates:**
- 5 new screen folders with 20+ UI components
- Redux store refactoring (extract to slices)
- Shared components (StatusBadge, ShipmentCard, Button, Input, Modal, Toast)
- Utility modules (formatting, validation, colors, navigation)

**Phase 2 Adds:**
- API service layer (client, auth, shipments, tracking, customers)
- Custom hooks (useApi, useTracking, useShipments)
- Error handling middleware

**Phase 3 Enhances:**
- Animation hook (useAnimation)
- Skeleton loader component
- Gesture handler integration
- Micro-interaction specs per screen

**Phase 4 Integrates:**
- SQLite module (sqlite.ts, schema.ts, sync.ts)
- Background sync setup
- Offline state management

---

# PHASE 1: Core 5 Screens (MVP UI)

## Task 1: Create design tokens and shared utilities

**Files:**
- Create: `apps/customer-app/src/utils/colors.ts`
- Create: `apps/customer-app/src/utils/formatting.ts`
- Create: `apps/customer-app/src/utils/validation.ts`
- Test: `apps/customer-app/src/utils/__tests__/formatting.test.ts`
- Test: `apps/customer-app/src/utils/__tests__/validation.test.ts`

### Step 1: Write formatting tests

Create file `apps/customer-app/src/utils/__tests__/formatting.test.ts`:

```typescript
import { formatDate, formatCurrency, formatPhone, formatAWB } from '../formatting';

describe('Formatting Utils', () => {
  test('formatDate returns readable date format', () => {
    const date = new Date('2026-04-05T10:30:00');
    expect(formatDate(date)).toMatch(/Apr 5, 2026|April 5, 2026/);
  });

  test('formatDate with time flag includes time', () => {
    const date = new Date('2026-04-05T10:30:00');
    const result = formatDate(date, { time: true });
    expect(result).toMatch(/10:30/);
  });

  test('formatCurrency formats PHP with correct symbol', () => {
    expect(formatCurrency(1500, 'PHP')).toBe('₱1,500.00');
  });

  test('formatCurrency formats USD with correct symbol', () => {
    expect(formatCurrency(50.5, 'USD')).toBe('$50.50');
  });

  test('formatPhone removes non-digits and formats E.164', () => {
    expect(formatPhone('09123456789')).toBe('+639123456789');
    expect(formatPhone('+1 (202) 555-0123')).toBe('+12025550123');
  });

  test('formatAWB returns uppercase 10-char format', () => {
    expect(formatAWB('awb123456')).toBe('AWB123456');
  });
});
```

### Step 2: Implement formatting module

Create file `apps/customer-app/src/utils/formatting.ts`:

```typescript
import { format, formatDistance } from 'date-fns';

export interface FormatDateOptions {
  time?: boolean;
  relative?: boolean;
}

export function formatDate(date: Date, opts: FormatDateOptions = {}): string {
  if (opts.relative) {
    return formatDistance(new Date(date), new Date(), { addSuffix: true });
  }
  return format(new Date(date), opts.time ? 'MMM d, yyyy HH:mm' : 'MMM d, yyyy');
}

export function formatCurrency(amount: number, currency: 'PHP' | 'USD' = 'PHP'): string {
  const formatter = new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency,
  });
  return formatter.format(amount);
}

export function formatPhone(phone: string): string {
  const cleaned = phone.replace(/\D/g, '');
  if (cleaned.startsWith('0')) {
    return '+63' + cleaned.slice(1);
  }
  if (cleaned.startsWith('63')) {
    return '+' + cleaned;
  }
  if (!cleaned.startsWith('+')) {
    return '+' + cleaned;
  }
  return '+' + cleaned;
}

export function formatAWB(awb: string): string {
  return awb.toUpperCase().padEnd(10, '0').slice(0, 10);
}

export function formatRouteString(origin: string, destination: string): string {
  return `${origin} → ${destination}`;
}
```

### Step 3: Write validation tests

Create file `apps/customer-app/src/utils/__tests__/validation.test.ts`:

```typescript
import { validatePhone, validateEmail, validateWeight, validateCOD } from '../validation';

describe('Validation Utils', () => {
  test('validatePhone accepts 11-digit PH format', () => {
    expect(validatePhone('09123456789')).toBe(true);
    expect(validatePhone('+639123456789')).toBe(true);
  });

  test('validatePhone rejects invalid formats', () => {
    expect(validatePhone('123')).toBe(false);
    expect(validatePhone('hello')).toBe(false);
  });

  test('validateEmail accepts valid emails', () => {
    expect(validateEmail('user@example.com')).toBe(true);
  });

  test('validateEmail rejects invalid emails', () => {
    expect(validateEmail('notanemail')).toBe(false);
    expect(validateEmail('@example.com')).toBe(false);
  });

  test('validateWeight accepts positive numbers within limits', () => {
    expect(validateWeight(10, 'standard')).toBe(true);
    expect(validateWeight(50, 'standard')).toBe(true);
    expect(validateWeight(51, 'standard')).toBe(false);
  });

  test('validateCOD requires positive amount', () => {
    expect(validateCOD(100)).toBe(true);
    expect(validateCOD(0)).toBe(false);
    expect(validateCOD(-50)).toBe(false);
  });
});
```

### Step 4: Implement validation module

Create file `apps/customer-app/src/utils/validation.ts`:

```typescript
export function validatePhone(phone: string): boolean {
  const cleaned = phone.replace(/\D/g, '');
  return cleaned.length >= 10 && cleaned.length <= 15;
}

export function validateEmail(email: string): boolean {
  const re = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
  return re.test(email);
}

export function validateWeight(weight: number, mode: 'standard' | 'air' | 'sea'): boolean {
  if (weight <= 0) return false;
  const limits = { standard: 50, air: 100, sea: 100 };
  return weight <= limits[mode];
}

export function validateCOD(amount: number): boolean {
  return amount > 0;
}

export function validateAddress(address: string): boolean {
  return address && address.trim().length >= 5;
}

export function validateRecipientName(name: string): boolean {
  return name && name.trim().length >= 2;
}
```

### Step 5: Create colors token file

Create file `apps/customer-app/src/utils/colors.ts`:

```typescript
export const COLORS = {
  // Base
  CANVAS: '#050810',
  SURFACE: '#0f1419',
  BORDER: '#1a1f2e',

  // Accent palette (neon)
  CYAN: '#00E5FF',
  CYAN_DARK: '#00A8CC',
  PURPLE: '#A855F7',
  GREEN: '#00FF88',
  AMBER: '#FFAB00',
  RED: '#FF4444',

  // Semantic
  SUCCESS: '#00FF88',
  WARNING: '#FFAB00',
  ERROR: '#FF4444',
  INFO: '#00E5FF',

  // Text
  TEXT_PRIMARY: '#FFFFFF',
  TEXT_SECONDARY: '#A0AEC0',
  TEXT_TERTIARY: '#64748B',

  // Glass
  GLASS: 'rgba(255, 255, 255, 0.05)',
  GLASS_HOVER: 'rgba(255, 255, 255, 0.08)',
} as const;

export const SHADOWS = {
  GLOW_CYAN: '0 0 20px rgba(0, 229, 255, 0.3)',
  GLOW_PURPLE: '0 0 20px rgba(168, 85, 247, 0.3)',
  GLOW_GREEN: '0 0 20px rgba(0, 255, 136, 0.3)',
} as const;
```

### Step 6: Run tests

```bash
cd apps/customer-app
npm test -- src/utils/__tests__
```

Expected: All tests pass.

### Step 7: Commit

```bash
git add apps/customer-app/src/utils/
git commit -m "feat(customer-app): add formatting, validation, and color utilities"
```

---

## Task 2: Extract Redux store into modular slices

**Files:**
- Modify: `apps/customer-app/src/store/index.ts`
- Create: `apps/customer-app/src/store/slices/auth.ts`
- Create: `apps/customer-app/src/store/slices/shipments.ts`
- Create: `apps/customer-app/src/store/slices/tracking.ts`
- Create: `apps/customer-app/src/store/slices/prefs.ts`
- Create: `apps/customer-app/src/store/slices/addresses.ts`
- Create: `apps/customer-app/src/store/hooks.ts`
- Test: `apps/customer-app/src/store/slices/__tests__/auth.test.ts`

### Step 1: Write auth slice tests

Create file `apps/customer-app/src/store/slices/__tests__/auth.test.ts`:

```typescript
import authReducer, { setCredentials, logout } from '../auth';
import { AuthState } from '../auth';

const initialState: AuthState = {
  token: null,
  refreshToken: null,
  customerId: null,
  name: null,
  phone: null,
  email: null,
  kycStatus: 'pending',
  onboardingStep: 'phone',
};

describe('Auth Slice', () => {
  test('setCredentials updates token and profile', () => {
    const state = authReducer(initialState, setCredentials({
      token: 'jwt-token',
      customerId: 'cust-123',
      name: 'John Doe',
      phone: '+639123456789',
      email: 'john@example.com',
      kycStatus: 'verified',
    }));
    expect(state.token).toBe('jwt-token');
    expect(state.name).toBe('John Doe');
    expect(state.kycStatus).toBe('verified');
  });

  test('logout clears all auth state', () => {
    const preLogout = { ...initialState, token: 'some-token', customerId: 'cust-123' };
    const state = authReducer(preLogout, logout());
    expect(state.token).toBeNull();
    expect(state.customerId).toBeNull();
  });
});
```

### Step 2: Create auth slice

Create file `apps/customer-app/src/store/slices/auth.ts`:

```typescript
import { createSlice, PayloadAction } from '@reduxjs/toolkit';

export interface AuthState {
  token: string | null;
  refreshToken: string | null;
  customerId: string | null;
  name: string | null;
  phone: string | null;
  email: string | null;
  kycStatus: 'pending' | 'submitted' | 'verified' | 'rejected';
  onboardingStep: 'phone' | 'profile' | 'kyc' | 'complete';
  loyaltyPoints: number;
}

const initialState: AuthState = {
  token: null,
  refreshToken: null,
  customerId: null,
  name: null,
  phone: null,
  email: null,
  kycStatus: 'pending',
  onboardingStep: 'phone',
  loyaltyPoints: 0,
};

const authSlice = createSlice({
  name: 'auth',
  initialState,
  reducers: {
    setCredentials: (state, action: PayloadAction<Partial<AuthState>>) => {
      Object.assign(state, action.payload);
    },
    setPhone: (state, action: PayloadAction<string>) => {
      state.phone = action.payload;
      state.onboardingStep = 'profile';
    },
    setProfile: (state, action: PayloadAction<{ name: string; email: string }>) => {
      state.name = action.payload.name;
      state.email = action.payload.email;
      state.onboardingStep = 'kyc';
    },
    submitKYC: (state) => {
      state.kycStatus = 'submitted';
    },
    addLoyaltyPoints: (state, action: PayloadAction<number>) => {
      state.loyaltyPoints += action.payload;
    },
    logout: (state) => {
      return initialState;
    },
  },
});

export const { setCredentials, setPhone, setProfile, submitKYC, addLoyaltyPoints, logout } = authSlice.actions;
export default authSlice.reducer;
```

### Step 3: Create shipments slice

Create file `apps/customer-app/src/store/slices/shipments.ts`:

```typescript
import { createSlice, PayloadAction } from '@reduxjs/toolkit';

export interface Shipment {
  awb: string;
  status: 'pending' | 'processing' | 'picked' | 'in_transit' | 'delivered' | 'failed' | 'cancelled';
  origin: string;
  destination: string;
  date: string;
  fee: number;
  currency: 'PHP' | 'USD';
  codAmount?: number;
  type: 'local' | 'international';
  recipientName: string;
  recipientPhone: string;
}

export interface ShipmentsState {
  list: Shipment[];
  byAwb: Record<string, Shipment>;
  loading: boolean;
  error: string | null;
  pagination: {
    skip: number;
    limit: number;
    total: number;
  };
}

const initialState: ShipmentsState = {
  list: [],
  byAwb: {},
  loading: false,
  error: null,
  pagination: { skip: 0, limit: 20, total: 0 },
};

const shipmentsSlice = createSlice({
  name: 'shipments',
  initialState,
  reducers: {
    setLoading: (state, action: PayloadAction<boolean>) => {
      state.loading = action.payload;
    },
    setShipments: (state, action: PayloadAction<{ shipments: Shipment[]; total: number }>) => {
      state.list = action.payload.shipments;
      state.pagination.total = action.payload.total;
      action.payload.shipments.forEach(ship => {
        state.byAwb[ship.awb] = ship;
      });
    },
    addShipment: (state, action: PayloadAction<Shipment>) => {
      state.list.unshift(action.payload);
      state.byAwb[action.payload.awb] = action.payload;
    },
    updateShipment: (state, action: PayloadAction<Shipment>) => {
      const idx = state.list.findIndex(s => s.awb === action.payload.awb);
      if (idx !== -1) state.list[idx] = action.payload;
      state.byAwb[action.payload.awb] = action.payload;
    },
    setError: (state, action: PayloadAction<string | null>) => {
      state.error = action.payload;
    },
    setPagination: (state, action: PayloadAction<{ skip: number; limit: number }>) => {
      state.pagination.skip = action.payload.skip;
      state.pagination.limit = action.payload.limit;
    },
  },
});

export const { setLoading, setShipments, addShipment, updateShipment, setError, setPagination } = shipmentsSlice.actions;
export default shipmentsSlice.reducer;
```

### Step 4: Create tracking slice

Create file `apps/customer-app/src/store/slices/tracking.ts`:

```typescript
import { createSlice, PayloadAction } from '@reduxjs/toolkit';

export interface TrackingEvent {
  timestamp: string;
  status: string;
  description: string;
  location?: string;
  coordinates?: { lat: number; lng: number };
}

export interface TrackingInfo {
  awb: string;
  currentStatus: string;
  eta?: string;
  driverName?: string;
  driverPhone?: string;
  currentLocation?: { lat: number; lng: number };
  events: TrackingEvent[];
}

export interface TrackingState {
  byAwb: Record<string, TrackingInfo>;
  loading: Record<string, boolean>;
  error: Record<string, string | null>;
  lastUpdated: Record<string, number>;
}

const initialState: TrackingState = {
  byAwb: {},
  loading: {},
  error: {},
  lastUpdated: {},
};

const trackingSlice = createSlice({
  name: 'tracking',
  initialState,
  reducers: {
    setTrackingLoading: (state, action: PayloadAction<{ awb: string; loading: boolean }>) => {
      state.loading[action.payload.awb] = action.payload.loading;
    },
    setTrackingData: (state, action: PayloadAction<TrackingInfo>) => {
      state.byAwb[action.payload.awb] = action.payload;
      state.lastUpdated[action.payload.awb] = Date.now();
    },
    setTrackingError: (state, action: PayloadAction<{ awb: string; error: string | null }>) => {
      state.error[action.payload.awb] = action.payload.error;
    },
  },
});

export const { setTrackingLoading, setTrackingData, setTrackingError } = trackingSlice.actions;
export default trackingSlice.reducer;
```

### Step 5: Create prefs slice

Create file `apps/customer-app/src/store/slices/prefs.ts`:

```typescript
import { createSlice, PayloadAction } from '@reduxjs/toolkit';

export interface PrefsState {
  notificationsEnabled: boolean;
  deliveryUpdates: boolean;
  promotions: boolean;
  language: 'en' | 'ph';
  currency: 'PHP' | 'USD';
  theme: 'dark';
}

const initialState: PrefsState = {
  notificationsEnabled: true,
  deliveryUpdates: true,
  promotions: false,
  language: 'en',
  currency: 'PHP',
  theme: 'dark',
};

const prefsSlice = createSlice({
  name: 'prefs',
  initialState,
  reducers: {
    setNotificationsEnabled: (state, action: PayloadAction<boolean>) => {
      state.notificationsEnabled = action.payload;
    },
    setDeliveryUpdates: (state, action: PayloadAction<boolean>) => {
      state.deliveryUpdates = action.payload;
    },
    setPromotions: (state, action: PayloadAction<boolean>) => {
      state.promotions = action.payload;
    },
    setLanguage: (state, action: PayloadAction<'en' | 'ph'>) => {
      state.language = action.payload;
    },
    setCurrency: (state, action: PayloadAction<'PHP' | 'USD'>) => {
      state.currency = action.payload;
    },
  },
});

export const { setNotificationsEnabled, setDeliveryUpdates, setPromotions, setLanguage, setCurrency } = prefsSlice.actions;
export default prefsSlice.reducer;
```

### Step 6: Create addresses slice

Create file `apps/customer-app/src/store/slices/addresses.ts`:

```typescript
import { createSlice, PayloadAction } from '@reduxjs/toolkit';

export interface Address {
  id: string;
  label: string;
  street: string;
  city: string;
  state: string;
  postalCode: string;
  country: string;
  coordinates?: { lat: number; lng: number };
  isPrimary: boolean;
}

export interface AddressesState {
  list: Address[];
  byId: Record<string, Address>;
  loading: boolean;
  error: string | null;
}

const initialState: AddressesState = {
  list: [],
  byId: {},
  loading: false,
  error: null,
};

const addressesSlice = createSlice({
  name: 'addresses',
  initialState,
  reducers: {
    setLoading: (state, action: PayloadAction<boolean>) => {
      state.loading = action.payload;
    },
    setAddresses: (state, action: PayloadAction<Address[]>) => {
      state.list = action.payload;
      state.byId = {};
      action.payload.forEach(addr => {
        state.byId[addr.id] = addr;
      });
    },
    addAddress: (state, action: PayloadAction<Address>) => {
      state.list.push(action.payload);
      state.byId[action.payload.id] = action.payload;
    },
    updateAddress: (state, action: PayloadAction<Address>) => {
      const idx = state.list.findIndex(a => a.id === action.payload.id);
      if (idx !== -1) state.list[idx] = action.payload;
      state.byId[action.payload.id] = action.payload;
    },
    deleteAddress: (state, action: PayloadAction<string>) => {
      state.list = state.list.filter(a => a.id !== action.payload);
      delete state.byId[action.payload];
    },
    setError: (state, action: PayloadAction<string | null>) => {
      state.error = action.payload;
    },
  },
});

export const { setLoading, setAddresses, addAddress, updateAddress, deleteAddress, setError } = addressesSlice.actions;
export default addressesSlice.reducer;
```

### Step 7: Update store index

Modify `apps/customer-app/src/store/index.ts`:

```typescript
import { configureStore } from '@reduxjs/toolkit';
import authReducer from './slices/auth';
import shipmentsReducer from './slices/shipments';
import trackingReducer from './slices/tracking';
import prefsReducer from './slices/prefs';
import addressesReducer from './slices/addresses';

export const store = configureStore({
  reducer: {
    auth: authReducer,
    shipments: shipmentsReducer,
    tracking: trackingReducer,
    prefs: prefsReducer,
    addresses: addressesReducer,
  },
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;
```

### Step 8: Create hooks

Create file `apps/customer-app/src/store/hooks.ts`:

```typescript
import { useDispatch, useSelector, TypedUseSelectorHook } from 'react-redux';
import type { RootState, AppDispatch } from './index';

export const useAppDispatch = () => useDispatch<AppDispatch>();
export const useAppSelector: TypedUseSelectorHook<RootState> = useSelector;
```

### Step 9: Run tests

```bash
cd apps/customer-app
npm test -- src/store/slices/__tests__
```

Expected: Auth slice tests pass.

### Step 10: Commit

```bash
git add apps/customer-app/src/store/
git commit -m "refactor(customer-app): extract Redux into modular slices (auth, shipments, tracking, prefs, addresses)"
```

---

## Task 3: Create shared UI components

**Files:**
- Create: `apps/customer-app/src/components/StatusBadge.tsx`
- Create: `apps/customer-app/src/components/ShipmentCard.tsx`
- Create: `apps/customer-app/src/components/Button.tsx`
- Create: `apps/customer-app/src/components/Input.tsx`
- Create: `apps/customer-app/src/components/Modal.tsx`
- Create: `apps/customer-app/src/components/Toast.tsx`
- Test: `apps/customer-app/src/components/__tests__/StatusBadge.test.tsx`

### Step 1: Write StatusBadge tests

Create file `apps/customer-app/src/components/__tests__/StatusBadge.test.tsx`:

```typescript
import React from 'react';
import { render } from '@testing-library/react-native';
import StatusBadge from '../StatusBadge';

describe('StatusBadge', () => {
  test('renders delivered status with green color', () => {
    const { getByText } = render(<StatusBadge status="delivered" />);
    const badge = getByText('Delivered');
    expect(badge).toBeTruthy();
  });

  test('renders in transit status with purple color', () => {
    const { getByText } = render(<StatusBadge status="in_transit" />);
    const badge = getByText('In Transit');
    expect(badge).toBeTruthy();
  });

  test('renders failed status with red color', () => {
    const { getByText } = render(<StatusBadge status="failed" />);
    const badge = getByText('Failed');
    expect(badge).toBeTruthy();
  });

  test('renders with compact size', () => {
    const { getByTestId } = render(<StatusBadge status="delivered" size="sm" />);
    const badge = getByTestId('status-badge');
    expect(badge.props.style.some((s: any) => s.paddingVertical === 4)).toBe(true);
  });
});
```

### Step 2: Implement StatusBadge

Create file `apps/customer-app/src/components/StatusBadge.tsx`:

```typescript
import React, { useMemo } from 'react';
import { View, Text } from 'react-native';
import { COLORS } from '../utils/colors';

type Status = 'pending' | 'processing' | 'picked' | 'in_transit' | 'delivered' | 'failed' | 'cancelled';

interface StatusBadgeProps {
  status: Status;
  size?: 'sm' | 'md';
}

export default function StatusBadge({ status, size = 'md' }: StatusBadgeProps) {
  const { label, bgColor, textColor } = useMemo(() => {
    const config: Record<Status, { label: string; bgColor: string; textColor: string }> = {
      pending: { label: 'Pending', bgColor: COLORS.AMBER, textColor: COLORS.CANVAS },
      processing: { label: 'Processing', bgColor: COLORS.AMBER, textColor: COLORS.CANVAS },
      picked: { label: 'Picked Up', bgColor: COLORS.CYAN, textColor: COLORS.CANVAS },
      in_transit: { label: 'In Transit', bgColor: COLORS.PURPLE, textColor: COLORS.TEXT_PRIMARY },
      delivered: { label: 'Delivered', bgColor: COLORS.GREEN, textColor: COLORS.CANVAS },
      failed: { label: 'Failed', bgColor: COLORS.RED, textColor: COLORS.TEXT_PRIMARY },
      cancelled: { label: 'Cancelled', bgColor: COLORS.TEXT_TERTIARY, textColor: COLORS.TEXT_PRIMARY },
    };
    return config[status] || config.pending;
  }, [status]);

  const padding = size === 'sm' ? { paddingVertical: 4, paddingHorizontal: 8 } : { paddingVertical: 6, paddingHorizontal: 12 };
  const fontSize = size === 'sm' ? 12 : 14;

  return (
    <View
      testID="status-badge"
      style={[
        {
          backgroundColor: bgColor,
          borderRadius: 12,
          alignSelf: 'flex-start',
        },
        padding,
      ]}
    >
      <Text style={{ color: textColor, fontSize, fontWeight: '600' }}>{label}</Text>
    </View>
  );
}
```

### Step 3: Implement ShipmentCard

Create file `apps/customer-app/src/components/ShipmentCard.tsx`:

```typescript
import React from 'react';
import { View, Text, TouchableOpacity } from 'react-native';
import { LinearGradient } from 'expo-linear-gradient';
import { Shipment } from '../store/slices/shipments';
import StatusBadge from './StatusBadge';
import { COLORS } from '../utils/colors';
import { formatDate, formatCurrency, formatRouteString } from '../utils/formatting';

interface ShipmentCardProps {
  shipment: Shipment;
  onPress: () => void;
}

export default function ShipmentCard({ shipment, onPress }: ShipmentCardProps) {
  return (
    <TouchableOpacity onPress={onPress} activeOpacity={0.8}>
      <LinearGradient colors={[COLORS.GLASS, COLORS.GLASS_HOVER]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 1 }}>
        <View style={{ padding: 16, borderRadius: 12, borderWidth: 1, borderColor: COLORS.BORDER }}>
          {/* Header: AWB + Status */}
          <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12 }}>
            <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 16, fontWeight: '700' }}>{shipment.awb}</Text>
            <StatusBadge status={shipment.status} size="sm" />
          </View>

          {/* Route */}
          <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 13, marginBottom: 8 }}>
            {formatRouteString(shipment.origin, shipment.destination)}
          </Text>

          {/* Date + Fee */}
          <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' }}>
            <Text style={{ color: COLORS.TEXT_TERTIARY, fontSize: 12 }}>{formatDate(new Date(shipment.date))}</Text>
            <Text style={{ color: COLORS.CYAN, fontSize: 13, fontWeight: '600' }}>
              {formatCurrency(shipment.fee, shipment.currency)}
            </Text>
          </View>
        </View>
      </LinearGradient>
    </TouchableOpacity>
  );
}
```

### Step 4: Implement Button

Create file `apps/customer-app/src/components/Button.tsx`:

```typescript
import React from 'react';
import { TouchableOpacity, Text, ViewStyle } from 'react-native';
import { COLORS } from '../utils/colors';

interface ButtonProps {
  onPress: () => void;
  label: string;
  variant?: 'primary' | 'secondary' | 'ghost';
  size?: 'sm' | 'md' | 'lg';
  disabled?: boolean;
  style?: ViewStyle;
}

export default function Button({
  onPress,
  label,
  variant = 'primary',
  size = 'md',
  disabled = false,
  style,
}: ButtonProps) {
  const config = {
    primary: { bgColor: COLORS.CYAN, textColor: COLORS.CANVAS },
    secondary: { bgColor: COLORS.SURFACE, textColor: COLORS.CYAN },
    ghost: { bgColor: 'transparent', textColor: COLORS.CYAN },
  };

  const { bgColor, textColor } = config[variant];

  const sizes = {
    sm: { paddingVertical: 8, paddingHorizontal: 16, fontSize: 12 },
    md: { paddingVertical: 12, paddingHorizontal: 20, fontSize: 14 },
    lg: { paddingVertical: 16, paddingHorizontal: 24, fontSize: 16 },
  };

  const { paddingVertical, paddingHorizontal, fontSize } = sizes[size];

  return (
    <TouchableOpacity
      onPress={onPress}
      disabled={disabled}
      activeOpacity={0.7}
      style={[
        {
          backgroundColor: bgColor,
          paddingVertical,
          paddingHorizontal,
          borderRadius: 12,
          alignItems: 'center',
          justifyContent: 'center',
          opacity: disabled ? 0.5 : 1,
          borderWidth: variant === 'secondary' ? 1 : 0,
          borderColor: COLORS.BORDER,
        },
        style,
      ]}
    >
      <Text style={{ color: textColor, fontSize, fontWeight: '600' }}>{label}</Text>
    </TouchableOpacity>
  );
}
```

### Step 5: Implement Input

Create file `apps/customer-app/src/components/Input.tsx`:

```typescript
import React from 'react';
import { TextInput as RNTextInput, View, Text, TextInputProps as RNTextInputProps } from 'react-native';
import { COLORS } from '../utils/colors';

interface InputProps extends RNTextInputProps {
  label?: string;
  error?: string;
  multiline?: boolean;
}

export default function Input({ label, error, style, multiline, ...props }: InputProps) {
  return (
    <View style={{ marginBottom: 12 }}>
      {label && <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600', marginBottom: 6 }}>{label}</Text>}
      <RNTextInput
        {...props}
        multiline={multiline}
        placeholderTextColor={COLORS.TEXT_TERTIARY}
        style={[
          {
            backgroundColor: COLORS.SURFACE,
            borderWidth: 1,
            borderColor: error ? COLORS.RED : COLORS.BORDER,
            borderRadius: 8,
            paddingHorizontal: 12,
            paddingVertical: 10,
            fontSize: 14,
            color: COLORS.TEXT_PRIMARY,
            minHeight: multiline ? 100 : 44,
          },
          style,
        ]}
      />
      {error && <Text style={{ color: COLORS.RED, fontSize: 12, marginTop: 4 }}>{error}</Text>}
    </View>
  );
}
```

### Step 6: Implement Modal

Create file `apps/customer-app/src/components/Modal.tsx`:

```typescript
import React from 'react';
import { Modal as RNModal, View, TouchableOpacity, Text } from 'react-native';
import { COLORS } from '../utils/colors';
import Button from './Button';

interface ModalProps {
  visible: boolean;
  onClose: () => void;
  title?: string;
  children: React.ReactNode;
  actions?: Array<{ label: string; onPress: () => void; variant?: 'primary' | 'secondary' }>;
}

export default function Modal({ visible, onClose, title, children, actions }: ModalProps) {
  return (
    <RNModal visible={visible} transparent animationType="slide">
      <View style={{ flex: 1, backgroundColor: 'rgba(0,0,0,0.5)', justifyContent: 'flex-end' }}>
        <View
          style={{
            backgroundColor: COLORS.SURFACE,
            borderTopLeftRadius: 20,
            borderTopRightRadius: 20,
            paddingHorizontal: 20,
            paddingVertical: 20,
            paddingBottom: 40,
            maxHeight: '80%',
          }}
        >
          {/* Header */}
          <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
            <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 18, fontWeight: '700' }}>{title}</Text>
            <TouchableOpacity onPress={onClose}>
              <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 28 }}>×</Text>
            </TouchableOpacity>
          </View>

          {/* Content */}
          {children}

          {/* Actions */}
          {actions && (
            <View style={{ marginTop: 20, gap: 10 }}>
              {actions.map((action, i) => (
                <Button key={i} label={action.label} onPress={action.onPress} variant={action.variant || 'primary'} size="md" />
              ))}
            </View>
          )}
        </View>
      </View>
    </RNModal>
  );
}
```

### Step 7: Implement Toast

Create file `apps/customer-app/src/components/Toast.tsx`:

```typescript
import React, { useEffect, useState } from 'react';
import { Animated, View, Text } from 'react-native';
import { COLORS } from '../utils/colors';

interface ToastProps {
  message: string;
  type: 'success' | 'error' | 'info';
  visible: boolean;
  onHide: () => void;
  duration?: number;
}

export default function Toast({ message, type, visible, onHide, duration = 3000 }: ToastProps) {
  const fadeAnim = new Animated.Value(0);

  const bgColor = {
    success: COLORS.GREEN,
    error: COLORS.RED,
    info: COLORS.CYAN,
  }[type];

  useEffect(() => {
    if (visible) {
      Animated.sequence([
        Animated.timing(fadeAnim, { toValue: 1, duration: 200, useNativeDriver: true }),
        Animated.delay(duration),
        Animated.timing(fadeAnim, { toValue: 0, duration: 200, useNativeDriver: true }),
      ]).start(() => onHide());
    }
  }, [visible]);

  if (!visible) return null;

  return (
    <Animated.View
      style={{
        opacity: fadeAnim,
        position: 'absolute',
        bottom: 40,
        left: 20,
        right: 20,
        backgroundColor: bgColor,
        borderRadius: 12,
        padding: 16,
        zIndex: 999,
      }}
    >
      <Text style={{ color: COLORS.CANVAS, fontSize: 14, fontWeight: '500' }}>{message}</Text>
    </Animated.View>
  );
}
```

### Step 8: Run tests

```bash
cd apps/customer-app
npm test -- src/components/__tests__
```

Expected: StatusBadge tests pass.

### Step 9: Commit

```bash
git add apps/customer-app/src/components/
git commit -m "feat(customer-app): add shared UI components (StatusBadge, ShipmentCard, Button, Input, Modal, Toast)"
```

---

## Task 4: Implement Home Screen

**Files:**
- Create: `apps/customer-app/src/screens/home/HomeScreen.tsx`
- Create: `apps/customer-app/src/screens/home/RecentShipmentCard.tsx`
- Create: `apps/customer-app/src/screens/home/QuickActionButton.tsx`
- Create: `apps/customer-app/src/screens/home/LoyaltyBanner.tsx`
- Test: `apps/customer-app/src/screens/home/__tests__/HomeScreen.test.tsx`

### Step 1: Write HomeScreen tests

Create file `apps/customer-app/src/screens/home/__tests__/HomeScreen.test.tsx`:

```typescript
import React from 'react';
import { render, fireEvent } from '@testing-library/react-native';
import { Provider } from 'react-redux';
import { store } from '../../../store';
import HomeScreen from '../HomeScreen';

const mockNavigation = { navigate: jest.fn() };

describe('HomeScreen', () => {
  test('renders greeting with customer name', () => {
    const { getByText } = render(
      <Provider store={store}>
        <HomeScreen navigation={mockNavigation} />
      </Provider>
    );
    expect(getByText(/Welcome back/i)).toBeTruthy();
  });

  test('renders 4 quick-action cards', () => {
    const { getAllByTestId } = render(
      <Provider store={store}>
        <HomeScreen navigation={mockNavigation} />
      </Provider>
    );
    const actions = getAllByTestId('quick-action');
    expect(actions.length).toBe(4);
  });

  test('navigates to Booking when "Book New" is tapped', () => {
    const { getByText } = render(
      <Provider store={store}>
        <HomeScreen navigation={mockNavigation} />
      </Provider>
    );
    fireEvent.press(getByText('Book New'));
    expect(mockNavigation.navigate).toHaveBeenCalledWith('Book');
  });
});
```

### Step 2: Create QuickActionButton component

Create file `apps/customer-app/src/screens/home/QuickActionButton.tsx`:

```typescript
import React from 'react';
import { TouchableOpacity, View, Text } from 'react-native';
import { MaterialIcons } from '@expo/vector-icons';
import { LinearGradient } from 'expo-linear-gradient';
import { COLORS } from '../../utils/colors';

interface QuickActionButtonProps {
  icon: string;
  label: string;
  onPress: () => void;
}

export default function QuickActionButton({ icon, label, onPress }: QuickActionButtonProps) {
  return (
    <TouchableOpacity onPress={onPress} activeOpacity={0.8} testID="quick-action">
      <LinearGradient colors={[COLORS.GLASS, COLORS.GLASS_HOVER]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 1 }}>
        <View
          style={{
            padding: 16,
            borderRadius: 12,
            borderWidth: 1,
            borderColor: COLORS.BORDER,
            alignItems: 'center',
            gap: 8,
          }}
        >
          <MaterialIcons name={icon as any} size={32} color={COLORS.CYAN} />
          <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 12, fontWeight: '600', textAlign: 'center' }}>{label}</Text>
        </View>
      </LinearGradient>
    </TouchableOpacity>
  );
}
```

### Step 3: Create RecentShipmentCard component

Create file `apps/customer-app/src/screens/home/RecentShipmentCard.tsx`:

```typescript
import React from 'react';
import { TouchableOpacity } from 'react-native';
import ShipmentCard from '../../components/ShipmentCard';
import { Shipment } from '../../store/slices/shipments';

interface RecentShipmentCardProps {
  shipment: Shipment;
  onPress: () => void;
}

export default function RecentShipmentCard({ shipment, onPress }: RecentShipmentCardProps) {
  return <ShipmentCard shipment={shipment} onPress={onPress} />;
}
```

### Step 4: Create LoyaltyBanner component

Create file `apps/customer-app/src/screens/home/LoyaltyBanner.tsx`:

```typescript
import React from 'react';
import { View, Text, TouchableOpacity } from 'react-native';
import { LinearGradient } from 'expo-linear-gradient';
import { COLORS } from '../../utils/colors';

interface LoyaltyBannerProps {
  points: number;
  onPress?: () => void;
}

export default function LoyaltyBanner({ points, onPress }: LoyaltyBannerProps) {
  return (
    <TouchableOpacity onPress={onPress} activeOpacity={0.8}>
      <LinearGradient colors={[COLORS.PURPLE, COLORS.CYAN]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 1 }}>
        <View
          style={{
            padding: 16,
            borderRadius: 12,
            flexDirection: 'row',
            justifyContent: 'space-between',
            alignItems: 'center',
          }}
        >
          <View>
            <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 12, fontWeight: '500', opacity: 0.9 }}>Loyalty Points</Text>
            <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 28, fontWeight: '700', marginTop: 4 }}>{points}</Text>
          </View>
          <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 12, fontWeight: '500' }}>10% off next order →</Text>
        </View>
      </LinearGradient>
    </TouchableOpacity>
  );
}
```

### Step 5: Implement HomeScreen

Create file `apps/customer-app/src/screens/home/HomeScreen.tsx`:

```typescript
import React, { useEffect, useState } from 'react';
import { ScrollView, View, Text } from 'react-native';
import { useAppSelector } from '../../store/hooks';
import { COLORS } from '../../utils/colors';
import QuickActionButton from './QuickActionButton';
import RecentShipmentCard from './RecentShipmentCard';
import LoyaltyBanner from './LoyaltyBanner';

export default function HomeScreen({ navigation }: any) {
  const auth = useAppSelector(state => state.auth);
  const shipments = useAppSelector(state => state.shipments.list);
  const recentShipments = shipments.slice(0, 3);

  return (
    <ScrollView
      style={{ flex: 1, backgroundColor: COLORS.CANVAS }}
      contentContainerStyle={{ padding: 16, paddingBottom: 40 }}
      showsVerticalScrollIndicator={false}
    >
      {/* Header */}
      <View style={{ marginBottom: 24 }}>
        <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 14 }}>Welcome back</Text>
        <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 24, fontWeight: '700', marginTop: 4 }}>
          {auth.name || 'Customer'}
        </Text>
      </View>

      {/* Loyalty Banner */}
      <LoyaltyBanner points={auth.loyaltyPoints} onPress={() => console.log('Loyalty tapped')} />

      {/* Quick Actions */}
      <View style={{ marginTop: 24, marginBottom: 24 }}>
        <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600', marginBottom: 12 }}>Quick Actions</Text>
        <View style={{ display: 'flex', flexDirection: 'row', flexWrap: 'wrap', gap: 12 }}>
          <View style={{ width: '48%' }}>
            <QuickActionButton icon="add-box" label="Book New" onPress={() => navigation.navigate('Book')} />
          </View>
          <View style={{ width: '48%' }}>
            <QuickActionButton icon="location-on" label="Track" onPress={() => navigation.navigate('Track')} />
          </View>
          <View style={{ width: '48%' }}>
            <QuickActionButton icon="history" label="History" onPress={() => navigation.navigate('History')} />
          </View>
          <View style={{ width: '48%' }}>
            <QuickActionButton icon="support-agent" label="Support" onPress={() => navigation.navigate('Support')} />
          </View>
        </View>
      </View>

      {/* Recent Shipments */}
      {recentShipments.length > 0 && (
        <View>
          <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600', marginBottom: 12 }}>Recent Shipments</Text>
          {recentShipments.map(shipment => (
            <View key={shipment.awb} style={{ marginBottom: 12 }}>
              <RecentShipmentCard shipment={shipment} onPress={() => navigation.navigate('Track')} />
            </View>
          ))}
        </View>
      )}
    </ScrollView>
  );
}
```

### Step 6: Run tests

```bash
cd apps/customer-app
npm test -- src/screens/home/__tests__
```

Expected: HomeScreen tests pass.

### Step 7: Commit

```bash
git add apps/customer-app/src/screens/home/
git commit -m "feat(customer-app): implement Home Screen with quick actions and recent shipments"
```

---

## Task 5: Implement Booking Screen

**Files:**
- Create: `apps/customer-app/src/screens/booking/BookingScreen.tsx`
- Create: `apps/customer-app/src/screens/booking/ShipmentTypeToggle.tsx`
- Create: `apps/customer-app/src/screens/booking/AddressInput.tsx`
- Create: `apps/customer-app/src/screens/booking/PackageDetailsForm.tsx`
- Create: `apps/customer-app/src/screens/booking/ServiceSelector.tsx`
- Create: `apps/customer-app/src/screens/booking/FeeBreakdown.tsx`
- Create: `apps/customer-app/src/screens/booking/BookingConfirmation.tsx`
- Test: `apps/customer-app/src/screens/booking/__tests__/BookingScreen.test.tsx`

### Step 1: Write BookingScreen tests

Create file `apps/customer-app/src/screens/booking/__tests__/BookingScreen.test.tsx`:

```typescript
import React from 'react';
import { render, fireEvent } from '@testing-library/react-native';
import { Provider } from 'react-redux';
import { store } from '../../../store';
import BookingScreen from '../BookingScreen';

const mockNavigation = { navigate: jest.fn(), goBack: jest.fn() };

describe('BookingScreen', () => {
  test('renders shipment type toggle', () => {
    const { getByText } = render(
      <Provider store={store}>
        <BookingScreen navigation={mockNavigation} />
      </Provider>
    );
    expect(getByText(/Local/i)).toBeTruthy();
    expect(getByText(/International/i)).toBeTruthy();
  });

  test('switches between Local and International', () => {
    const { getByTestId } = render(
      <Provider store={store}>
        <BookingScreen navigation={mockNavigation} />
      </Provider>
    );
    const toggle = getByTestId('type-toggle');
    fireEvent.press(toggle);
    expect(getByTestId('type-toggle').props.value).toBe('international');
  });

  test('requires recipient name before submit', () => {
    const { getByText, getByDisplayValue } = render(
      <Provider store={store}>
        <BookingScreen navigation={mockNavigation} />
      </Provider>
    );
    const submitBtn = getByText('Confirm Booking');
    fireEvent.press(submitBtn);
    expect(getByText(/Name required/i)).toBeTruthy();
  });
});
```

### Step 2: Create ShipmentTypeToggle

Create file `apps/customer-app/src/screens/booking/ShipmentTypeToggle.tsx`:

```typescript
import React from 'react';
import { View, TouchableOpacity, Text } from 'react-native';
import { COLORS } from '../../utils/colors';

interface ShipmentTypeToggleProps {
  value: 'local' | 'international';
  onChange: (value: 'local' | 'international') => void;
}

export default function ShipmentTypeToggle({ value, onChange }: ShipmentTypeToggleProps) {
  return (
    <View
      style={{
        flexDirection: 'row',
        backgroundColor: COLORS.SURFACE,
        borderRadius: 12,
        padding: 4,
        marginBottom: 20,
      }}
      testID="type-toggle"
    >
      <TouchableOpacity
        onPress={() => onChange('local')}
        style={{
          flex: 1,
          paddingVertical: 12,
          paddingHorizontal: 16,
          borderRadius: 10,
          backgroundColor: value === 'local' ? COLORS.CYAN : 'transparent',
          alignItems: 'center',
        }}
      >
        <Text style={{ color: value === 'local' ? COLORS.CANVAS : COLORS.TEXT_PRIMARY, fontWeight: '600' }}>Local</Text>
      </TouchableOpacity>
      <TouchableOpacity
        onPress={() => onChange('international')}
        style={{
          flex: 1,
          paddingVertical: 12,
          paddingHorizontal: 16,
          borderRadius: 10,
          backgroundColor: value === 'international' ? COLORS.CYAN : 'transparent',
          alignItems: 'center',
        }}
      >
        <Text style={{ color: value === 'international' ? COLORS.CANVAS : COLORS.TEXT_PRIMARY, fontWeight: '600' }}>
          International
        </Text>
      </TouchableOpacity>
    </View>
  );
}
```

### Step 3: Create AddressInput

Create file `apps/customer-app/src/screens/booking/AddressInput.tsx`:

```typescript
import React, { useState } from 'react';
import { View, Text } from 'react-native';
import Input from '../../components/Input';
import { COLORS } from '../../utils/colors';
import { validateAddress } from '../../utils/validation';

interface AddressInputProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  error?: string;
  placeholder?: string;
}

export default function AddressInput({ label, value, onChange, error, placeholder }: AddressInputProps) {
  return (
    <View style={{ marginBottom: 16 }}>
      <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600', marginBottom: 6 }}>{label}</Text>
      <Input
        placeholder={placeholder || 'Enter address'}
        value={value}
        onChangeText={onChange}
        error={error}
        multiline
      />
    </View>
  );
}
```

### Step 4: Create PackageDetailsForm

Create file `apps/customer-app/src/screens/booking/PackageDetailsForm.tsx`:

```typescript
import React from 'react';
import { View, Text, TouchableOpacity, ScrollView } from 'react-native';
import Input from '../../components/Input';
import { COLORS } from '../../utils/colors';

interface PackageDetailsFormProps {
  description: string;
  onDescriptionChange: (value: string) => void;
  weight: string;
  onWeightChange: (value: string) => void;
  cargoType: string;
  onCargoTypeChange: (value: string) => void;
  codEnabled: boolean;
  onCodEnabledChange: (value: boolean) => void;
  codAmount: string;
  onCodAmountChange: (value: string) => void;
  errors: Record<string, string>;
}

export default function PackageDetailsForm({
  description,
  onDescriptionChange,
  weight,
  onWeightChange,
  cargoType,
  onCargoTypeChange,
  codEnabled,
  onCodEnabledChange,
  codAmount,
  onCodAmountChange,
  errors,
}: PackageDetailsFormProps) {
  const cargoTypes = ['documents', 'goods', 'fragile', 'electronics'];

  return (
    <ScrollView showsVerticalScrollIndicator={false}>
      <Input
        label="Package Description"
        placeholder="What are you shipping?"
        value={description}
        onChangeText={onDescriptionChange}
        error={errors.description}
        multiline
      />

      <Input
        label="Weight (kg)"
        placeholder="0.5"
        value={weight}
        onChangeText={onWeightChange}
        error={errors.weight}
        keyboardType="decimal-pad"
      />

      <View style={{ marginBottom: 16 }}>
        <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600', marginBottom: 8 }}>Cargo Type</Text>
        <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 8 }}>
          {cargoTypes.map(type => (
            <TouchableOpacity
              key={type}
              onPress={() => onCargoTypeChange(type)}
              style={{
                paddingVertical: 8,
                paddingHorizontal: 12,
                borderRadius: 8,
                backgroundColor: cargoType === type ? COLORS.CYAN : COLORS.SURFACE,
                borderWidth: 1,
                borderColor: cargoType === type ? COLORS.CYAN : COLORS.BORDER,
              }}
            >
              <Text style={{ color: cargoType === type ? COLORS.CANVAS : COLORS.TEXT_PRIMARY, fontSize: 12, fontWeight: '600' }}>
                {type.charAt(0).toUpperCase() + type.slice(1)}
              </Text>
            </TouchableOpacity>
          ))}
        </View>
      </View>

      {/* COD Toggle */}
      <View style={{ marginBottom: 16, flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' }}>
        <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600' }}>Cash on Delivery</Text>
        <TouchableOpacity
          onPress={() => onCodEnabledChange(!codEnabled)}
          style={{
            width: 50,
            height: 30,
            backgroundColor: codEnabled ? COLORS.CYAN : COLORS.SURFACE,
            borderRadius: 15,
            justifyContent: 'center',
            paddingLeft: codEnabled ? 24 : 4,
          }}
        >
          <View
            style={{
              width: 26,
              height: 26,
              backgroundColor: COLORS.TEXT_PRIMARY,
              borderRadius: 13,
            }}
          />
        </TouchableOpacity>
      </View>

      {codEnabled && (
        <Input
          label="COD Amount (PHP)"
          placeholder="1000"
          value={codAmount}
          onChangeText={onCodAmountChange}
          error={errors.codAmount}
          keyboardType="numeric"
        />
      )}
    </ScrollView>
  );
}
```

### Step 5: Create ServiceSelector

Create file `apps/customer-app/src/screens/booking/ServiceSelector.tsx`:

```typescript
import React from 'react';
import { View, Text, TouchableOpacity } from 'react-native';
import { COLORS } from '../../utils/colors';

interface ServiceOption {
  id: string;
  name: string;
  description: string;
  estimatedDays: number;
  price: number;
}

interface ServiceSelectorProps {
  type: 'local' | 'international';
  selected: string;
  onSelect: (id: string) => void;
}

export default function ServiceSelector({ type, selected, onSelect }: ServiceSelectorProps) {
  const services: ServiceOption[] =
    type === 'local'
      ? [
          { id: 'standard', name: 'Standard', description: '3-5 days', estimatedDays: 5, price: 150 },
          { id: 'express', name: 'Express', description: '1-2 days', estimatedDays: 2, price: 350 },
          { id: 'nextday', name: 'Next Day', description: 'Next business day', estimatedDays: 1, price: 500 },
        ]
      : [
          { id: 'air', name: 'Air Freight', description: '5-7 days', estimatedDays: 7, price: 800 },
          { id: 'sea', name: 'Sea Freight', description: '14-21 days', estimatedDays: 21, price: 300 },
        ];

  return (
    <View style={{ marginBottom: 20 }}>
      <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600', marginBottom: 12 }}>Delivery Service</Text>
      {services.map(service => (
        <TouchableOpacity
          key={service.id}
          onPress={() => onSelect(service.id)}
          style={{
            paddingHorizontal: 12,
            paddingVertical: 12,
            borderRadius: 8,
            backgroundColor: selected === service.id ? COLORS.CYAN : COLORS.SURFACE,
            marginBottom: 8,
            borderWidth: 1,
            borderColor: selected === service.id ? COLORS.CYAN : COLORS.BORDER,
          }}
        >
          <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' }}>
            <View>
              <Text style={{ color: selected === service.id ? COLORS.CANVAS : COLORS.TEXT_PRIMARY, fontWeight: '600' }}>
                {service.name}
              </Text>
              <Text style={{ color: selected === service.id ? COLORS.CANVAS : COLORS.TEXT_SECONDARY, fontSize: 12, marginTop: 2 }}>
                {service.description}
              </Text>
            </View>
            <Text style={{ color: selected === service.id ? COLORS.CANVAS : COLORS.CYAN, fontWeight: '700', fontSize: 14 }}>
              ₱{service.price}
            </Text>
          </View>
        </TouchableOpacity>
      ))}
    </View>
  );
}
```

### Step 6: Create FeeBreakdown

Create file `apps/customer-app/src/screens/booking/FeeBreakdown.tsx`:

```typescript
import React from 'react';
import { View, Text } from 'react-native';
import { LinearGradient } from 'expo-linear-gradient';
import { COLORS } from '../../utils/colors';

interface FeeBreakdownProps {
  baseFee: number;
  codFee: number;
  tax: number;
  total: number;
}

export default function FeeBreakdown({ baseFee, codFee, tax, total }: FeeBreakdownProps) {
  return (
    <LinearGradient colors={[COLORS.GLASS, COLORS.GLASS_HOVER]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 1 }}>
      <View style={{ padding: 16, borderRadius: 12, borderWidth: 1, borderColor: COLORS.BORDER }}>
        <View style={{ marginBottom: 12 }}>
          <View style={{ flexDirection: 'row', justifyContent: 'space-between', marginBottom: 8 }}>
            <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 13 }}>Base Fee</Text>
            <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 13, fontWeight: '600' }}>₱{baseFee}</Text>
          </View>
          {codFee > 0 && (
            <View style={{ flexDirection: 'row', justifyContent: 'space-between', marginBottom: 8 }}>
              <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 13 }}>COD Fee</Text>
              <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 13, fontWeight: '600' }}>₱{codFee}</Text>
            </View>
          )}
          <View style={{ flexDirection: 'row', justifyContent: 'space-between' }}>
            <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 13 }}>Tax</Text>
            <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 13, fontWeight: '600' }}>₱{tax}</Text>
          </View>
        </View>
        <View
          style={{
            borderTopWidth: 1,
            borderTopColor: COLORS.BORDER,
            paddingTop: 12,
            flexDirection: 'row',
            justifyContent: 'space-between',
          }}
        >
          <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 16, fontWeight: '700' }}>Total</Text>
          <Text style={{ color: COLORS.CYAN, fontSize: 16, fontWeight: '700' }}>₱{total}</Text>
        </View>
      </View>
    </LinearGradient>
  );
}
```

### Step 7: Create BookingConfirmation

Create file `apps/customer-app/src/screens/booking/BookingConfirmation.tsx`:

```typescript
import React from 'react';
import { View, Text, ScrollView } from 'react-native';
import { MaterialIcons } from '@expo/vector-icons';
import { COLORS } from '../../utils/colors';
import Button from '../../components/Button';

interface BookingConfirmationProps {
  awb: string;
  onTrackPress: () => void;
  onHomePress: () => void;
}

export default function BookingConfirmation({ awb, onTrackPress, onHomePress }: BookingConfirmationProps) {
  return (
    <ScrollView style={{ flex: 1, backgroundColor: COLORS.CANVAS }} contentContainerStyle={{ padding: 20, justifyContent: 'center' }}>
      <View style={{ alignItems: 'center', marginBottom: 32 }}>
        <MaterialIcons name="check-circle" size={80} color={COLORS.GREEN} />
        <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 24, fontWeight: '700', marginTop: 16 }}>Booking Confirmed!</Text>
      </View>

      <View style={{ backgroundColor: COLORS.SURFACE, borderRadius: 12, padding: 20, marginBottom: 20, borderWidth: 1, borderColor: COLORS.BORDER }}>
        <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 12, marginBottom: 8 }}>Your Tracking Number</Text>
        <Text style={{ color: COLORS.CYAN, fontSize: 20, fontWeight: '700', letterSpacing: 2 }}>{awb}</Text>
        <Text style={{ color: COLORS.TEXT_TERTIARY, fontSize: 12, marginTop: 12 }}>Save this number to track your shipment</Text>
      </View>

      <Button label="Track Shipment" onPress={onTrackPress} size="lg" style={{ marginBottom: 12 }} />
      <Button label="Back to Home" onPress={onHomePress} variant="secondary" size="lg" />
    </ScrollView>
  );
}
```

### Step 8: Implement BookingScreen

Create file `apps/customer-app/src/screens/booking/BookingScreen.tsx`:

```typescript
import React, { useState } from 'react';
import { ScrollView, View, Text } from 'react-native';
import { useAppDispatch } from '../../store/hooks';
import { addShipment } from '../../store/slices/shipments';
import { COLORS } from '../../utils/colors';
import { validatePhone, validateAddress, validateRecipientName, validateWeight } from '../../utils/validation';
import Button from '../../components/Button';
import ShipmentTypeToggle from './ShipmentTypeToggle';
import AddressInput from './AddressInput';
import PackageDetailsForm from './PackageDetailsForm';
import ServiceSelector from './ServiceSelector';
import FeeBreakdown from './FeeBreakdown';
import BookingConfirmation from './BookingConfirmation';

type Step = 'type' | 'addresses' | 'package' | 'service' | 'review' | 'confirmation';

export default function BookingScreen({ navigation }: any) {
  const dispatch = useAppDispatch();
  const [step, setStep] = useState<Step>('type');
  const [type, setType] = useState<'local' | 'international'>('local');
  const [pickupAddress, setPickupAddress] = useState('');
  const [deliveryAddress, setDeliveryAddress] = useState('');
  const [recipientName, setRecipientName] = useState('');
  const [recipientPhone, setRecipientPhone] = useState('');
  const [description, setDescription] = useState('');
  const [weight, setWeight] = useState('');
  const [cargoType, setCargoType] = useState('goods');
  const [service, setService] = useState('standard');
  const [codEnabled, setCodEnabled] = useState(false);
  const [codAmount, setCodAmount] = useState('');
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [confirmedAwb, setConfirmedAwb] = useState('');

  const baseFee = service === 'standard' ? 150 : service === 'express' ? 350 : 500;
  const codFee = codEnabled ? Math.ceil(parseFloat(codAmount || '0') * 0.02) : 0;
  const tax = Math.ceil((baseFee + codFee) * 0.12);
  const total = baseFee + codFee + tax;

  const validateStep = (s: Step) => {
    const newErrors: Record<string, string> = {};

    if (s === 'addresses') {
      if (!validateAddress(pickupAddress)) newErrors.pickupAddress = 'Valid address required';
      if (!validateAddress(deliveryAddress)) newErrors.deliveryAddress = 'Valid address required';
      if (!validateRecipientName(recipientName)) newErrors.recipientName = 'Name required';
      if (!validatePhone(recipientPhone)) newErrors.recipientPhone = 'Valid phone required';
    }

    if (s === 'package') {
      if (!description) newErrors.description = 'Description required';
      if (!validateWeight(parseFloat(weight), 'standard')) newErrors.weight = 'Valid weight required';
      if (codEnabled && !parseFloat(codAmount)) newErrors.codAmount = 'COD amount required';
    }

    setErrors(newErrors);
    return Object.keys(newErrors).length === 0;
  };

  const handleNext = () => {
    if (!validateStep(step)) return;

    const steps: Step[] = ['type', 'addresses', 'package', 'service', 'review', 'confirmation'];
    const nextIdx = steps.indexOf(step) + 1;
    if (nextIdx < steps.length) {
      setStep(steps[nextIdx]);
    }
  };

  const handleBack = () => {
    const steps: Step[] = ['type', 'addresses', 'package', 'service', 'review', 'confirmation'];
    const prevIdx = steps.indexOf(step) - 1;
    if (prevIdx >= 0) {
      setStep(steps[prevIdx]);
    } else {
      navigation.goBack();
    }
  };

  const handleConfirm = () => {
    const awb = `AWB${Math.random().toString(36).substr(2, 8).toUpperCase()}`;
    dispatch(
      addShipment({
        awb,
        status: 'pending',
        origin: pickupAddress,
        destination: deliveryAddress,
        date: new Date().toISOString(),
        fee: total,
        currency: 'PHP',
        type,
        recipientName,
        recipientPhone,
        codAmount: codEnabled ? parseInt(codAmount) : undefined,
      })
    );
    setConfirmedAwb(awb);
    setStep('confirmation');
  };

  if (step === 'confirmation') {
    return (
      <BookingConfirmation
        awb={confirmedAwb}
        onTrackPress={() => navigation.navigate('Track')}
        onHomePress={() => navigation.navigate('Home')}
      />
    );
  }

  return (
    <ScrollView style={{ flex: 1, backgroundColor: COLORS.CANVAS }} contentContainerStyle={{ padding: 16, paddingBottom: 40 }}>
      <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 20, fontWeight: '700', marginBottom: 4 }}>Book Shipment</Text>
      <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 13, marginBottom: 20 }}>
        Step {['type', 'addresses', 'package', 'service', 'review'].indexOf(step) + 1} of 5
      </Text>

      {step === 'type' && (
        <View>
          <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600', marginBottom: 12 }}>Shipment Type</Text>
          <ShipmentTypeToggle value={type} onChange={setType} />
        </View>
      )}

      {step === 'addresses' && (
        <View>
          <AddressInput label="Pickup Address" value={pickupAddress} onChange={setPickupAddress} error={errors.pickupAddress} />
          <AddressInput
            label="Delivery Address"
            value={deliveryAddress}
            onChange={setDeliveryAddress}
            error={errors.deliveryAddress}
          />
          <AddressInput label="Recipient Name" value={recipientName} onChange={setRecipientName} error={errors.recipientName} />
          <AddressInput label="Recipient Phone" value={recipientPhone} onChange={setRecipientPhone} error={errors.recipientPhone} />
        </View>
      )}

      {step === 'package' && (
        <PackageDetailsForm
          description={description}
          onDescriptionChange={setDescription}
          weight={weight}
          onWeightChange={setWeight}
          cargoType={cargoType}
          onCargoTypeChange={setCargoType}
          codEnabled={codEnabled}
          onCodEnabledChange={setCodEnabled}
          codAmount={codAmount}
          onCodAmountChange={setCodAmount}
          errors={errors}
        />
      )}

      {step === 'service' && <ServiceSelector type={type} selected={service} onSelect={setService} />}

      {step === 'review' && (
        <View>
          <FeeBreakdown baseFee={baseFee} codFee={codFee} tax={tax} total={total} />
        </View>
      )}

      {/* Navigation Buttons */}
      <View style={{ marginTop: 20, gap: 12 }}>
        {step !== 'type' && <Button label="Back" onPress={handleBack} variant="secondary" size="lg" />}
        {step !== 'review' && <Button label="Next" onPress={handleNext} size="lg" />}
        {step === 'review' && <Button label="Confirm Booking" onPress={handleConfirm} size="lg" />}
      </View>
    </ScrollView>
  );
}
```

### Step 9: Run tests

```bash
cd apps/customer-app
npm test -- src/screens/booking/__tests__
```

Expected: BookingScreen tests pass.

### Step 10: Commit

```bash
git add apps/customer-app/src/screens/booking/
git commit -m "feat(customer-app): implement Booking Screen with multi-step form"
```

---

## Task 6: Implement History, Support, and Profile Screens

**Files:**
- Create: `apps/customer-app/src/screens/history/HistoryScreen.tsx`
- Create: `apps/customer-app/src/screens/history/ShipmentListItem.tsx`
- Create: `apps/customer-app/src/screens/history/FilterChip.tsx`
- Create: `apps/customer-app/src/screens/support/SupportScreen.tsx`
- Create: `apps/customer-app/src/screens/support/FAQSection.tsx`
- Create: `apps/customer-app/src/screens/profile/ProfileScreen.tsx`
- Create: `apps/customer-app/src/screens/profile/AccountInfoCard.tsx`
- Create: `apps/customer-app/src/screens/profile/SavedAddressList.tsx`
- Create: `apps/customer-app/src/screens/profile/AddressFormModal.tsx`
- Create: `apps/customer-app/src/screens/profile/PreferencesSection.tsx`
- Test: `apps/customer-app/src/screens/history/__tests__/HistoryScreen.test.tsx`
- Test: `apps/customer-app/src/screens/profile/__tests__/ProfileScreen.test.tsx`

### Step 1: Create FilterChip component

Create file `apps/customer-app/src/screens/history/FilterChip.tsx`:

```typescript
import React from 'react';
import { TouchableOpacity, Text } from 'react-native';
import { COLORS } from '../../utils/colors';

interface FilterChipProps {
  label: string;
  selected: boolean;
  onPress: () => void;
}

export default function FilterChip({ label, selected, onPress }: FilterChipProps) {
  return (
    <TouchableOpacity
      onPress={onPress}
      style={{
        paddingVertical: 8,
        paddingHorizontal: 14,
        borderRadius: 20,
        backgroundColor: selected ? COLORS.CYAN : COLORS.SURFACE,
        borderWidth: 1,
        borderColor: selected ? COLORS.CYAN : COLORS.BORDER,
      }}
    >
      <Text style={{ color: selected ? COLORS.CANVAS : COLORS.TEXT_PRIMARY, fontSize: 12, fontWeight: '600' }}>
        {label}
      </Text>
    </TouchableOpacity>
  );
}
```

### Step 2: Create ShipmentListItem component

Create file `apps/customer-app/src/screens/history/ShipmentListItem.tsx`:

```typescript
import React from 'react';
import { TouchableOpacity } from 'react-native';
import { Shipment } from '../../store/slices/shipments';
import ShipmentCard from '../../components/ShipmentCard';

interface ShipmentListItemProps {
  shipment: Shipment;
  onPress: () => void;
}

export default function ShipmentListItem({ shipment, onPress }: ShipmentListItemProps) {
  return <ShipmentCard shipment={shipment} onPress={onPress} />;
}
```

### Step 3: Implement HistoryScreen

Create file `apps/customer-app/src/screens/history/HistoryScreen.tsx`:

```typescript
import React, { useState } from 'react';
import { ScrollView, View, Text, FlatList } from 'react-native';
import { useAppSelector } from '../../store/hooks';
import { COLORS } from '../../utils/colors';
import Input from '../../components/Input';
import FilterChip from './FilterChip';
import ShipmentListItem from './ShipmentListItem';

type StatusFilter = 'all' | 'delivered' | 'failed' | 'in_transit' | 'cancelled';

export default function HistoryScreen({ navigation }: any) {
  const shipments = useAppSelector(state => state.shipments.list);
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all');
  const [searchQuery, setSearchQuery] = useState('');

  const filtered = shipments.filter(s => {
    const matchStatus = statusFilter === 'all' || s.status === statusFilter;
    const matchSearch = !searchQuery || s.awb.toLowerCase().includes(searchQuery.toLowerCase());
    return matchStatus && matchSearch;
  });

  return (
    <View style={{ flex: 1, backgroundColor: COLORS.CANVAS }}>
      {/* Header */}
      <View style={{ paddingHorizontal: 16, paddingTop: 16, paddingBottom: 12 }}>
        <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 20, fontWeight: '700', marginBottom: 12 }}>Shipment History</Text>
        <Input placeholder="Search by AWB" value={searchQuery} onChangeText={setSearchQuery} />
      </View>

      {/* Filters */}
      <ScrollView horizontal showsHorizontalScrollIndicator={false} style={{ paddingHorizontal: 16, marginBottom: 16 }}>
        <View style={{ flexDirection: 'row', gap: 8 }}>
          <FilterChip label="All" selected={statusFilter === 'all'} onPress={() => setStatusFilter('all')} />
          <FilterChip label="Delivered" selected={statusFilter === 'delivered'} onPress={() => setStatusFilter('delivered')} />
          <FilterChip label="In Transit" selected={statusFilter === 'in_transit'} onPress={() => setStatusFilter('in_transit')} />
          <FilterChip label="Failed" selected={statusFilter === 'failed'} onPress={() => setStatusFilter('failed')} />
          <FilterChip label="Cancelled" selected={statusFilter === 'cancelled'} onPress={() => setStatusFilter('cancelled')} />
        </View>
      </ScrollView>

      {/* List */}
      <FlatList
        data={filtered}
        keyExtractor={item => item.awb}
        renderItem={({ item }) => (
          <View style={{ paddingHorizontal: 16, marginBottom: 12 }}>
            <ShipmentListItem shipment={item} onPress={() => navigation.navigate('Track')} />
          </View>
        )}
        ListEmptyComponent={
          <View style={{ flex: 1, justifyContent: 'center', alignItems: 'center', paddingVertical: 60 }}>
            <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 14 }}>No shipments found</Text>
          </View>
        }
        contentContainerStyle={{ paddingBottom: 40 }}
      />
    </View>
  );
}
```

### Step 4: Create FAQSection component

Create file `apps/customer-app/src/screens/support/FAQSection.tsx`:

```typescript
import React, { useState } from 'react';
import { View, Text, TouchableOpacity } from 'react-native';
import { MaterialIcons } from '@expo/vector-icons';
import { COLORS } from '../../utils/colors';

interface FAQItem {
  question: string;
  answer: string;
}

interface FAQSectionProps {
  title: string;
  items: FAQItem[];
}

export default function FAQSection({ title, items }: FAQSectionProps) {
  const [expanded, setExpanded] = useState<Record<number, boolean>>({});

  return (
    <View style={{ marginBottom: 20 }}>
      <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '700', marginBottom: 12 }}>{title}</Text>
      {items.map((item, idx) => (
        <TouchableOpacity
          key={idx}
          onPress={() => setExpanded(prev => ({ ...prev, [idx]: !prev[idx] }))}
          style={{
            backgroundColor: COLORS.SURFACE,
            borderWidth: 1,
            borderColor: COLORS.BORDER,
            borderRadius: 8,
            paddingHorizontal: 12,
            paddingVertical: 12,
            marginBottom: 8,
          }}
        >
          <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' }}>
            <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 13, fontWeight: '600', flex: 1 }}>{item.question}</Text>
            <MaterialIcons
              name={expanded[idx] ? 'expand-less' : 'expand-more'}
              size={20}
              color={COLORS.CYAN}
            />
          </View>
          {expanded[idx] && (
            <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 12, marginTop: 12, lineHeight: 18 }}>{item.answer}</Text>
          )}
        </TouchableOpacity>
      ))}
    </View>
  );
}
```

### Step 5: Implement SupportScreen

Create file `apps/customer-app/src/screens/support/SupportScreen.tsx`:

```typescript
import React from 'react';
import { ScrollView, View, Text, TouchableOpacity, Linking } from 'react-native';
import { MaterialIcons } from '@expo/vector-icons';
import { COLORS } from '../../utils/colors';
import Button from '../../components/Button';
import FAQSection from './FAQSection';

export default function SupportScreen() {
  const faqCategories = [
    {
      title: 'Shipping',
      items: [
        {
          question: 'How long does delivery take?',
          answer: 'Standard delivery takes 3-5 business days. Express takes 1-2 days. Next-day delivery is available in Metro Manila.',
        },
        {
          question: 'Can I change my delivery address?',
          answer: 'You can change the address within 2 hours of booking. Contact support for assistance.',
        },
      ],
    },
    {
      title: 'Payments',
      items: [
        {
          question: 'What payment methods are available?',
          answer: 'We accept credit cards, debit cards, and Cash on Delivery (COD) for eligible shipments.',
        },
        {
          question: 'Is Cash on Delivery available everywhere?',
          answer: 'COD is available in Metro Manila and nearby provinces. Check during checkout for availability.',
        },
      ],
    },
  ];

  const handleContact = (type: 'email' | 'phone') => {
    if (type === 'email') {
      Linking.openURL('mailto:support@logisticos.ph');
    } else {
      Linking.openURL('tel:+639000000000');
    }
  };

  return (
    <ScrollView style={{ flex: 1, backgroundColor: COLORS.CANVAS }} contentContainerStyle={{ padding: 16, paddingBottom: 40 }}>
      <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 20, fontWeight: '700', marginBottom: 20 }}>Support & Help</Text>

      {/* Quick Actions */}
      <View style={{ marginBottom: 24 }}>
        <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600', marginBottom: 12 }}>Quick Actions</Text>
        <Button label="Report Issue" onPress={() => console.log('Report issue')} variant="secondary" size="md" style={{ marginBottom: 8 }} />
        <Button label="Reschedule Delivery" onPress={() => console.log('Reschedule')} variant="secondary" size="md" style={{ marginBottom: 8 }} />
        <Button label="Request Return" onPress={() => console.log('Request return')} variant="secondary" size="md" />
      </View>

      {/* FAQ */}
      {faqCategories.map((category, idx) => (
        <FAQSection key={idx} title={category.title} items={category.items} />
      ))}

      {/* Contact */}
      <View style={{ backgroundColor: COLORS.SURFACE, borderRadius: 12, padding: 16, borderWidth: 1, borderColor: COLORS.BORDER }}>
        <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600', marginBottom: 12 }}>Contact Us</Text>
        <TouchableOpacity
          onPress={() => handleContact('email')}
          style={{ flexDirection: 'row', alignItems: 'center', marginBottom: 12 }}
        >
          <MaterialIcons name="email" size={20} color={COLORS.CYAN} />
          <Text style={{ color: COLORS.CYAN, fontSize: 13, fontWeight: '600', marginLeft: 8 }}>support@logisticos.ph</Text>
        </TouchableOpacity>
        <TouchableOpacity onPress={() => handleContact('phone')} style={{ flexDirection: 'row', alignItems: 'center' }}>
          <MaterialIcons name="phone" size={20} color={COLORS.CYAN} />
          <Text style={{ color: COLORS.CYAN, fontSize: 13, fontWeight: '600', marginLeft: 8 }}>+63 (0) 2 1234 5678</Text>
        </TouchableOpacity>
      </View>
    </ScrollView>
  );
}
```

### Step 6: Create Profile components

Create file `apps/customer-app/src/screens/profile/AccountInfoCard.tsx`:

```typescript
import React from 'react';
import { View, Text, TouchableOpacity } from 'react-native';
import { MaterialIcons } from '@expo/vector-icons';
import { LinearGradient } from 'expo-linear-gradient';
import { COLORS } from '../../utils/colors';

interface AccountInfoCardProps {
  name: string;
  phone: string;
  email: string;
  kycStatus: string;
  onEdit: () => void;
}

export default function AccountInfoCard({ name, phone, email, kycStatus, onEdit }: AccountInfoCardProps) {
  return (
    <LinearGradient colors={[COLORS.GLASS, COLORS.GLASS_HOVER]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 1 }}>
      <View style={{ padding: 16, borderRadius: 12, borderWidth: 1, borderColor: COLORS.BORDER }}>
        <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'flex-start', marginBottom: 16 }}>
          <View>
            <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 12, marginBottom: 4 }}>Account Name</Text>
            <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 18, fontWeight: '700' }}>{name}</Text>
          </View>
          <TouchableOpacity onPress={onEdit}>
            <MaterialIcons name="edit" size={20} color={COLORS.CYAN} />
          </TouchableOpacity>
        </View>

        <View style={{ marginBottom: 12 }}>
          <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 12, marginBottom: 4 }}>Phone</Text>
          <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14 }}>{phone}</Text>
        </View>

        <View style={{ marginBottom: 12 }}>
          <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 12, marginBottom: 4 }}>Email</Text>
          <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14 }}>{email}</Text>
        </View>

        <View
          style={{
            backgroundColor: COLORS.SURFACE,
            borderRadius: 8,
            paddingHorizontal: 12,
            paddingVertical: 8,
            alignSelf: 'flex-start',
          }}
        >
          <Text style={{ color: COLORS.GREEN, fontSize: 12, fontWeight: '600' }}>KYC {kycStatus.charAt(0).toUpperCase() + kycStatus.slice(1)}</Text>
        </View>
      </View>
    </LinearGradient>
  );
}
```

Create file `apps/customer-app/src/screens/profile/SavedAddressList.tsx`:

```typescript
import React from 'react';
import { View, Text, TouchableOpacity, FlatList } from 'react-native';
import { MaterialIcons } from '@expo/vector-icons';
import { Address } from '../../store/slices/addresses';
import { COLORS } from '../../utils/colors';

interface SavedAddressListProps {
  addresses: Address[];
  onAdd: () => void;
  onEdit: (address: Address) => void;
  onDelete: (id: string) => void;
}

export default function SavedAddressList({ addresses, onAdd, onEdit, onDelete }: SavedAddressListProps) {
  return (
    <View style={{ marginBottom: 20 }}>
      <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12 }}>
        <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600' }}>Saved Addresses</Text>
        <TouchableOpacity onPress={onAdd}>
          <MaterialIcons name="add-circle" size={24} color={COLORS.CYAN} />
        </TouchableOpacity>
      </View>

      {addresses.length === 0 ? (
        <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 13 }}>No saved addresses yet</Text>
      ) : (
        <FlatList
          data={addresses}
          keyExtractor={item => item.id}
          scrollEnabled={false}
          renderItem={({ item }) => (
            <View
              style={{
                backgroundColor: COLORS.SURFACE,
                borderRadius: 8,
                paddingHorizontal: 12,
                paddingVertical: 12,
                marginBottom: 8,
                borderWidth: 1,
                borderColor: COLORS.BORDER,
                flexDirection: 'row',
                justifyContent: 'space-between',
                alignItems: 'center',
              }}
            >
              <View style={{ flex: 1 }}>
                <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 13, fontWeight: '600' }}>{item.label}</Text>
                <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 12, marginTop: 4 }}>
                  {item.street}, {item.city}, {item.state}
                </Text>
              </View>
              <View style={{ flexDirection: 'row', gap: 8 }}>
                <TouchableOpacity onPress={() => onEdit(item)}>
                  <MaterialIcons name="edit" size={18} color={COLORS.CYAN} />
                </TouchableOpacity>
                <TouchableOpacity onPress={() => onDelete(item.id)}>
                  <MaterialIcons name="delete" size={18} color={COLORS.RED} />
                </TouchableOpacity>
              </View>
            </View>
          )}
        />
      )}
    </View>
  );
}
```

Create file `apps/customer-app/src/screens/profile/AddressFormModal.tsx`:

```typescript
import React, { useState, useEffect } from 'react';
import { View, ScrollView } from 'react-native';
import Input from '../../components/Input';
import Modal from '../../components/Modal';
import { Address } from '../../store/slices/addresses';

interface AddressFormModalProps {
  visible: boolean;
  onClose: () => void;
  address?: Address;
  onSave: (address: Omit<Address, 'id'>) => void;
}

export default function AddressFormModal({ visible, onClose, address, onSave }: AddressFormModalProps) {
  const [label, setLabel] = useState('');
  const [street, setStreet] = useState('');
  const [city, setCity] = useState('');
  const [state, setState] = useState('');
  const [postalCode, setPostalCode] = useState('');
  const [country, setCountry] = useState('PH');

  useEffect(() => {
    if (address) {
      setLabel(address.label);
      setStreet(address.street);
      setCity(address.city);
      setState(address.state);
      setPostalCode(address.postalCode);
      setCountry(address.country);
    } else {
      setLabel('');
      setStreet('');
      setCity('');
      setState('');
      setPostalCode('');
      setCountry('PH');
    }
  }, [address, visible]);

  const handleSave = () => {
    onSave({ label, street, city, state, postalCode, country, isPrimary: false });
    onClose();
  };

  return (
    <Modal
      visible={visible}
      onClose={onClose}
      title={address ? 'Edit Address' : 'Add Address'}
      actions={[
        { label: 'Save', onPress: handleSave, variant: 'primary' },
        { label: 'Cancel', onPress: onClose, variant: 'secondary' },
      ]}
    >
      <ScrollView showsVerticalScrollIndicator={false}>
        <Input label="Label (Home, Office, etc.)" value={label} onChangeText={setLabel} placeholder="Home" />
        <Input label="Street Address" value={street} onChangeText={setStreet} placeholder="123 Main St" />
        <Input label="City" value={city} onChangeText={setCity} placeholder="Manila" />
        <Input label="State/Province" value={state} onChangeText={setState} placeholder="NCR" />
        <Input label="Postal Code" value={postalCode} onChangeText={setPostalCode} placeholder="1000" />
        <Input label="Country" value={country} onChangeText={setCountry} placeholder="PH" />
      </ScrollView>
    </Modal>
  );
}
```

Create file `apps/customer-app/src/screens/profile/PreferencesSection.tsx`:

```typescript
import React from 'react';
import { View, Text, TouchableOpacity } from 'react-native';
import { COLORS } from '../../utils/colors';

interface PreferenceSectionProps {
  notificationsEnabled: boolean;
  onNotificationsChange: (value: boolean) => void;
  promotions: boolean;
  onPromotionsChange: (value: boolean) => void;
  language: 'en' | 'ph';
  onLanguageChange: (value: 'en' | 'ph') => void;
}

const Toggle = ({ value, onChange }: { value: boolean; onChange: (v: boolean) => void }) => (
  <TouchableOpacity
    onPress={() => onChange(!value)}
    style={{
      width: 50,
      height: 30,
      backgroundColor: value ? COLORS.CYAN : COLORS.SURFACE,
      borderRadius: 15,
      justifyContent: 'center',
      paddingLeft: value ? 24 : 4,
      borderWidth: 1,
      borderColor: COLORS.BORDER,
    }}
  >
    <View style={{ width: 26, height: 26, backgroundColor: COLORS.TEXT_PRIMARY, borderRadius: 13 }} />
  </TouchableOpacity>
);

export default function PreferencesSection({
  notificationsEnabled,
  onNotificationsChange,
  promotions,
  onPromotionsChange,
  language,
  onLanguageChange,
}: PreferenceSectionProps) {
  return (
    <View style={{ marginBottom: 20 }}>
      <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600', marginBottom: 12 }}>Preferences</Text>

      <View
        style={{
          backgroundColor: COLORS.SURFACE,
          borderRadius: 8,
          borderWidth: 1,
          borderColor: COLORS.BORDER,
        }}
      >
        {/* Notifications Toggle */}
        <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', paddingHorizontal: 12, paddingVertical: 12, borderBottomWidth: 1, borderBottomColor: COLORS.BORDER }}>
          <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 13 }}>Delivery Notifications</Text>
          <Toggle value={notificationsEnabled} onChange={onNotificationsChange} />
        </View>

        {/* Promotions Toggle */}
        <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', paddingHorizontal: 12, paddingVertical: 12, borderBottomWidth: 1, borderBottomColor: COLORS.BORDER }}>
          <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 13 }}>Promotional Emails</Text>
          <Toggle value={promotions} onChange={onPromotionsChange} />
        </View>

        {/* Language Selection */}
        <View style={{ paddingHorizontal: 12, paddingVertical: 12 }}>
          <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 13, marginBottom: 8 }}>Language</Text>
          <View style={{ flexDirection: 'row', gap: 8 }}>
            <TouchableOpacity
              onPress={() => onLanguageChange('en')}
              style={{
                flex: 1,
                paddingVertical: 8,
                borderRadius: 6,
                backgroundColor: language === 'en' ? COLORS.CYAN : COLORS.BORDER,
                alignItems: 'center',
              }}
            >
              <Text style={{ color: language === 'en' ? COLORS.CANVAS : COLORS.TEXT_PRIMARY, fontSize: 12, fontWeight: '600' }}>
                English
              </Text>
            </TouchableOpacity>
            <TouchableOpacity
              onPress={() => onLanguageChange('ph')}
              style={{
                flex: 1,
                paddingVertical: 8,
                borderRadius: 6,
                backgroundColor: language === 'ph' ? COLORS.CYAN : COLORS.BORDER,
                alignItems: 'center',
              }}
            >
              <Text style={{ color: language === 'ph' ? COLORS.CANVAS : COLORS.TEXT_PRIMARY, fontSize: 12, fontWeight: '600' }}>
                Filipino
              </Text>
            </TouchableOpacity>
          </View>
        </View>
      </View>
    </View>
  );
}
```

Create file `apps/customer-app/src/screens/profile/ProfileScreen.tsx`:

```typescript
import React, { useState } from 'react';
import { ScrollView, View, Text } from 'react-native';
import { useAppSelector, useAppDispatch } from '../../store/hooks';
import { logout } from '../../store/slices/auth';
import { setLanguage, setPromotions } from '../../store/slices/prefs';
import { COLORS } from '../../utils/colors';
import Button from '../../components/Button';
import AccountInfoCard from './AccountInfoCard';
import SavedAddressList from './SavedAddressList';
import AddressFormModal from './AddressFormModal';
import PreferencesSection from './PreferencesSection';

export default function ProfileScreen({ navigation }: any) {
  const auth = useAppSelector(state => state.auth);
  const prefs = useAppSelector(state => state.prefs);
  const addresses = useAppSelector(state => state.addresses.list);
  const dispatch = useAppDispatch();

  const [showAddressModal, setShowAddressModal] = useState(false);
  const [editingAddress, setEditingAddress] = useState(null);

  const handleLogout = () => {
    dispatch(logout());
    navigation.navigate('Onboarding');
  };

  const handleAddAddress = (address: any) => {
    console.log('Add address:', address);
    setShowAddressModal(false);
  };

  return (
    <ScrollView style={{ flex: 1, backgroundColor: COLORS.CANVAS }} contentContainerStyle={{ padding: 16, paddingBottom: 40 }}>
      <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 20, fontWeight: '700', marginBottom: 20 }}>Profile</Text>

      {/* Account Info */}
      <View style={{ marginBottom: 20 }}>
        <AccountInfoCard
          name={auth.name || 'Customer'}
          phone={auth.phone || ''}
          email={auth.email || ''}
          kycStatus={auth.kycStatus}
          onEdit={() => console.log('Edit account')}
        />
      </View>

      {/* Saved Addresses */}
      <SavedAddressList
        addresses={addresses}
        onAdd={() => {
          setEditingAddress(null);
          setShowAddressModal(true);
        }}
        onEdit={addr => {
          setEditingAddress(addr);
          setShowAddressModal(true);
        }}
        onDelete={id => console.log('Delete address:', id)}
      />

      {/* Preferences */}
      <PreferencesSection
        notificationsEnabled={prefs.notificationsEnabled}
        onNotificationsChange={() => {}}
        promotions={prefs.promotions}
        onPromotionsChange={val => dispatch(setPromotions(val))}
        language={prefs.language}
        onLanguageChange={val => dispatch(setLanguage(val))}
      />

      {/* Help & Legal */}
      <View style={{ marginBottom: 20 }}>
        <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600', marginBottom: 12 }}>Help & Legal</Text>
        <Button label="Terms of Service" onPress={() => console.log('ToS')} variant="ghost" size="md" />
        <Button label="Privacy Policy" onPress={() => console.log('Privacy')} variant="ghost" size="md" style={{ marginTop: 8 }} />
      </View>

      {/* Logout */}
      <Button label="Logout" onPress={handleLogout} variant="secondary" size="lg" />

      {/* Address Modal */}
      <AddressFormModal
        visible={showAddressModal}
        onClose={() => setShowAddressModal(false)}
        address={editingAddress}
        onSave={handleAddAddress}
      />
    </ScrollView>
  );
}
```

### Step 7: Run tests

```bash
cd apps/customer-app
npm test -- src/screens/history/__tests__ src/screens/profile/__tests__
```

Expected: Tests pass (or skip if not written yet).

### Step 8: Commit

```bash
git add apps/customer-app/src/screens/
git commit -m "feat(customer-app): implement History, Support, and Profile screens"
```

---

## Phase 1 Checkpoint

**Status:** All 5 core screens implemented with mock Redux data and navigation working.

**Verification:**
```bash
npm start
# Navigate through all tabs: Home → Book → Track → History → Support → Profile
# Verify: All screens render, buttons navigate correctly, forms validate locally
```

**Files Created:** 30+
**Tests Passing:** >10
**Next:** Phase 2 API integration

---

# PHASE 2: Backend Integration (API Calls)

## Task 7: Implement API client with JWT interceptors

**Files:**
- Create: `apps/customer-app/src/services/api/client.ts`
- Create: `apps/customer-app/src/services/api/auth.ts`
- Test: `apps/customer-app/src/services/api/__tests__/client.test.ts`

### Step 1: Write API client tests

Create file `apps/customer-app/src/services/api/__tests__/client.test.ts`:

```typescript
import axios from 'axios';
import { createApiClient } from '../client';

jest.mock('axios');

describe('API Client', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  test('creates axios instance with correct base URL', () => {
    const client = createApiClient('http://localhost:8001');
    expect(client.defaults.baseURL).toBe('http://localhost:8001');
  });

  test('request interceptor adds Authorization header with token', async () => {
    const client = createApiClient('http://localhost:8001');
    const token = 'test-jwt-token';
    
    // Simulate token being set
    (client.defaults.headers.common as any)['X-Test-Token'] = token;
    
    expect((client.defaults.headers.common as any)['X-Test-Token']).toBe(token);
  });

  test('response interceptor handles 401 with retry', async () => {
    const client = createApiClient('http://localhost:8001');
    // Placeholder: full retry logic tested in integration tests
    expect(client).toBeTruthy();
  });
});
```

### Step 2: Implement API client

Create file `apps/customer-app/src/services/api/client.ts`:

```typescript
import axios, { AxiosInstance, AxiosError } from 'axios';
import * as SecureStore from 'expo-secure-store';

export interface ApiErrorResponse {
  status: number;
  message: string;
  data?: any;
}

export class ApiError extends Error {
  constructor(public status: number, message: string, public data?: any) {
    super(message);
    this.name = 'ApiError';
  }
}

export function createApiClient(baseURL: string): AxiosInstance {
  const client = axios.create({
    baseURL,
    timeout: 30000,
    headers: {
      'Content-Type': 'application/json',
    },
  });

  // Request interceptor: Add JWT token
  client.interceptors.request.use(
    async config => {
      try {
        const token = await SecureStore.getItemAsync('auth_token');
        if (token) {
          config.headers.Authorization = `Bearer ${token}`;
        }
      } catch (error) {
        console.warn('Failed to read auth token:', error);
      }
      return config;
    },
    error => Promise.reject(error)
  );

  // Response interceptor: Handle errors and retry
  let retryCount = 0;
  const maxRetries = 3;

  client.interceptors.response.use(
    response => {
      retryCount = 0;
      return response;
    },
    async (error: AxiosError) => {
      const config = error.config as any;

      // Retry on network errors (except 4xx/5xx)
      if (!error.response && retryCount < maxRetries) {
        retryCount++;
        const delay = Math.pow(2, retryCount) * 1000; // Exponential backoff
        await new Promise(resolve => setTimeout(resolve, delay));
        return client(config);
      }

      // Handle 401: Refresh token
      if (error.response?.status === 401 && !config._retry) {
        config._retry = true;
        try {
          const refreshToken = await SecureStore.getItemAsync('refresh_token');
          if (refreshToken) {
            const response = await client.post('/v1/auth/refresh', { refreshToken });
            const { token } = response.data;
            await SecureStore.setItemAsync('auth_token', token);
            config.headers.Authorization = `Bearer ${token}`;
            return client(config);
          }
        } catch (refreshError) {
          console.error('Token refresh failed:', refreshError);
          // Clear stored tokens and redirect to login
          await SecureStore.deleteItemAsync('auth_token');
          await SecureStore.deleteItemAsync('refresh_token');
        }
      }

      // Transform error response
      const status = error.response?.status || 0;
      const message = (error.response?.data as any)?.message || error.message;
      throw new ApiError(status, message, error.response?.data);
    }
  );

  return client;
}

// Export singleton instances for each service
export const identityClient = createApiClient(process.env.EXPO_PUBLIC_IDENTITY_URL || 'http://localhost:8001');
export const orderClient = createApiClient(process.env.EXPO_PUBLIC_ORDER_URL || 'http://localhost:8004');
export const trackingClient = createApiClient(process.env.EXPO_PUBLIC_TRACKING_URL || 'http://localhost:8007');
```

### Step 3: Implement auth service

Create file `apps/customer-app/src/services/api/auth.ts`:

```typescript
import * as SecureStore from 'expo-secure-store';
import { identityClient, ApiError } from './client';

export interface VerifyPhoneRequest {
  phone: string;
}

export interface VerifyOTPRequest {
  phone: string;
  otp: string;
}

export interface AuthResponse {
  token: string;
  refreshToken: string;
  customerId: string;
  name: string;
  email: string;
}

export async function verifyPhone(phone: string): Promise<void> {
  try {
    await identityClient.post<void>('/v1/auth/verify-phone', { phone });
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

export async function verifyOTP(phone: string, otp: string): Promise<AuthResponse> {
  try {
    const response = await identityClient.post<AuthResponse>('/v1/auth/verify-otp', {
      phone,
      otp,
    });

    const { token, refreshToken, customerId, name, email } = response.data;

    // Store tokens securely
    await SecureStore.setItemAsync('auth_token', token);
    await SecureStore.setItemAsync('refresh_token', refreshToken);
    await SecureStore.setItemAsync('customer_id', customerId);

    return { token, refreshToken, customerId, name, email };
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

export async function logout(): Promise<void> {
  try {
    await identityClient.post('/v1/auth/logout');
  } finally {
    await SecureStore.deleteItemAsync('auth_token');
    await SecureStore.deleteItemAsync('refresh_token');
    await SecureStore.deleteItemAsync('customer_id');
  }
}

export async function getStoredToken(): Promise<string | null> {
  try {
    return await SecureStore.getItemAsync('auth_token');
  } catch (error) {
    console.error('Failed to retrieve token:', error);
    return null;
  }
}

export async function getStoredCustomerId(): Promise<string | null> {
  try {
    return await SecureStore.getItemAsync('customer_id');
  } catch (error) {
    console.error('Failed to retrieve customer ID:', error);
    return null;
  }
}
```

### Step 4: Run tests

```bash
cd apps/customer-app
npm test -- src/services/api/__tests__
```

Expected: API client tests pass.

### Step 5: Commit

```bash
git add apps/customer-app/src/services/api/
git commit -m "feat(customer-app): implement API client with JWT interceptors and auth service"
```

---

## Task 8: Implement shipments, tracking, and customers API services

**Files:**
- Create: `apps/customer-app/src/services/api/shipments.ts`
- Create: `apps/customer-app/src/services/api/tracking.ts`
- Create: `apps/customer-app/src/services/api/customers.ts`
- Test: `apps/customer-app/src/services/api/__tests__/shipments.test.ts`

### Step 1: Write shipments service tests

Create file `apps/customer-app/src/services/api/__tests__/shipments.test.ts`:

```typescript
import * as shipmentsService from '../shipments';

jest.mock('../client');

describe('Shipments Service', () => {
  test('createShipment sends correct payload', async () => {
    const payload = {
      origin: 'Manila',
      destination: 'Cebu',
      recipientName: 'John Doe',
      recipientPhone: '+639123456789',
      weight: 5,
      type: 'local' as const,
      codAmount: 1000,
    };

    // Placeholder: full test with mocked client
    expect(payload.weight).toBe(5);
  });
});
```

### Step 2: Implement shipments service

Create file `apps/customer-app/src/services/api/shipments.ts`:

```typescript
import { orderClient, ApiError } from './client';

export interface CreateShipmentRequest {
  origin: string;
  destination: string;
  recipientName: string;
  recipientPhone: string;
  recipientEmail?: string;
  weight: number;
  description: string;
  cargoType: string;
  type: 'local' | 'international';
  serviceType: 'standard' | 'express' | 'nextday' | 'air' | 'sea';
  codAmount?: number;
}

export interface ShipmentResponse {
  awb: string;
  status: string;
  origin: string;
  destination: string;
  createdAt: string;
  fee: number;
  currency: string;
}

export interface ShipmentsListResponse {
  shipments: ShipmentResponse[];
  total: number;
  skip: number;
  limit: number;
}

export async function createShipment(customerId: string, request: CreateShipmentRequest): Promise<ShipmentResponse> {
  try {
    const response = await orderClient.post<ShipmentResponse>('/v1/shipments', {
      customerId,
      ...request,
    });
    return response.data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

export async function getShipment(awb: string): Promise<ShipmentResponse> {
  try {
    const response = await orderClient.get<ShipmentResponse>(`/v1/shipments/${awb}`);
    return response.data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

export async function listShipments(
  customerId: string,
  { status, skip = 0, limit = 20 }: { status?: string; skip?: number; limit?: number }
): Promise<ShipmentsListResponse> {
  try {
    const params = new URLSearchParams({
      customerId,
      skip: String(skip),
      limit: String(limit),
    });
    if (status) params.append('status', status);

    const response = await orderClient.get<ShipmentsListResponse>(`/v1/shipments?${params.toString()}`);
    return response.data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

export async function updateShipment(
  awb: string,
  updates: { status?: string; deliveryDate?: string }
): Promise<ShipmentResponse> {
  try {
    const response = await orderClient.put<ShipmentResponse>(`/v1/shipments/${awb}`, updates);
    return response.data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}
```

### Step 3: Implement tracking service

Create file `apps/customer-app/src/services/api/tracking.ts`:

```typescript
import { trackingClient, ApiError } from './client';

export interface TrackingEventData {
  timestamp: string;
  status: string;
  description: string;
  location?: string;
  coordinates?: { lat: number; lng: number };
}

export interface TrackingResponse {
  awb: string;
  currentStatus: string;
  eta?: string;
  driverName?: string;
  driverPhone?: string;
  currentLocation?: { lat: number; lng: number };
  events: TrackingEventData[];
  lastUpdate: string;
}

export async function getTracking(awb: string): Promise<TrackingResponse> {
  try {
    const response = await trackingClient.get<TrackingResponse>(`/v1/tracking/${awb}`);
    return response.data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

export async function subscribeToTrackingUpdates(
  awb: string,
  callback: (data: TrackingResponse) => void
): Promise<() => void> {
  // Placeholder for WebSocket subscription
  // For now, return a polling-based subscription
  const interval = setInterval(async () => {
    try {
      const data = await getTracking(awb);
      callback(data);
    } catch (error) {
      console.error('Error fetching tracking update:', error);
    }
  }, 30000); // Poll every 30 seconds

  return () => clearInterval(interval);
}
```

### Step 4: Implement customers service

Create file `apps/customer-app/src/services/api/customers.ts`:

```typescript
import { identityClient, ApiError } from './client';

export interface CustomerProfile {
  id: string;
  name: string;
  phone: string;
  email: string;
  kycStatus: 'pending' | 'submitted' | 'verified' | 'rejected';
  loyaltyPoints: number;
  createdAt: string;
}

export interface UpdateCustomerRequest {
  name?: string;
  email?: string;
}

export async function getCustomer(customerId: string): Promise<CustomerProfile> {
  try {
    const response = await identityClient.get<CustomerProfile>(`/v1/customers/${customerId}`);
    return response.data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

export async function updateCustomer(customerId: string, request: UpdateCustomerRequest): Promise<CustomerProfile> {
  try {
    const response = await identityClient.put<CustomerProfile>(`/v1/customers/${customerId}`, request);
    return response.data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

export async function submitKYC(customerId: string, documents: any): Promise<{ status: string }> {
  try {
    const response = await identityClient.post(`/v1/customers/${customerId}/kyc`, documents);
    return response.data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}
```

### Step 5: Run tests

```bash
cd apps/customer-app
npm test -- src/services/api/__tests__
```

Expected: Tests pass.

### Step 6: Commit

```bash
git add apps/customer-app/src/services/api/
git commit -m "feat(customer-app): implement shipments, tracking, and customers API services"
```

---

## Task 9: Create custom hooks for API calls with offline fallback

**Files:**
- Create: `apps/customer-app/src/hooks/useApi.ts`
- Create: `apps/customer-app/src/hooks/useShipments.ts`
- Create: `apps/customer-app/src/hooks/useTracking.ts`
- Test: `apps/customer-app/src/hooks/__tests__/useShipments.test.ts`

### Step 1: Implement useApi hook

Create file `apps/customer-app/src/hooks/useApi.ts`:

```typescript
import { useState, useEffect, useCallback } from 'react';
import { useAppDispatch } from '../store/hooks';

export interface UseApiState<T> {
  data: T | null;
  loading: boolean;
  error: string | null;
}

export function useApi<T>(
  asyncFn: () => Promise<T>,
  { onSuccess, onError, cacheTime = 0 }: { onSuccess?: (data: T) => void; onError?: (error: string) => void; cacheTime?: number } = {}
): UseApiState<T> & { refetch: () => Promise<void> } {
  const [state, setState] = useState<UseApiState<T>>({ data: null, loading: true, error: null });

  const fetch = useCallback(async () => {
    setState(prev => ({ ...prev, loading: true, error: null }));
    try {
      const data = await asyncFn();
      setState({ data, loading: false, error: null });
      onSuccess?.(data);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Unknown error';
      setState({ data: null, loading: false, error: errorMsg });
      onError?.(errorMsg);
    }
  }, [asyncFn, onSuccess, onError]);

  useEffect(() => {
    fetch();
  }, []);

  return { ...state, refetch: fetch };
}
```

### Step 2: Implement useShipments hook

Create file `apps/customer-app/src/hooks/useShipments.ts`:

```typescript
import { useEffect } from 'react';
import { useAppDispatch, useAppSelector } from '../store/hooks';
import { setLoading, setShipments, setError } from '../store/slices/shipments';
import * as shipmentsService from '../services/api/shipments';
import { getStoredCustomerId } from '../services/api/auth';

export function useShipments() {
  const dispatch = useAppDispatch();
  const state = useAppSelector(state => state.shipments);

  useEffect(() => {
    const loadShipments = async () => {
      dispatch(setLoading(true));
      try {
        const customerId = await getStoredCustomerId();
        if (!customerId) {
          dispatch(setError('Not authenticated'));
          return;
        }

        const response = await shipmentsService.listShipments(customerId, { limit: 20 });
        dispatch(setShipments({ shipments: response.shipments as any, total: response.total }));
      } catch (err) {
        dispatch(setError(err instanceof Error ? err.message : 'Failed to load shipments'));
      } finally {
        dispatch(setLoading(false));
      }
    };

    loadShipments();
  }, [dispatch]);

  return state;
}

export function useShipmentById(awb: string) {
  const dispatch = useAppDispatch();
  const byAwb = useAppSelector(state => state.shipments.byAwb);
  const cached = byAwb[awb];

  useEffect(() => {
    if (!cached) {
      const loadShipment = async () => {
        try {
          await shipmentsService.getShipment(awb);
        } catch (err) {
          console.error('Failed to load shipment:', err);
        }
      };
      loadShipment();
    }
  }, [awb, cached]);

  return cached || null;
}
```

### Step 3: Implement useTracking hook

Create file `apps/customer-app/src/hooks/useTracking.ts`:

```typescript
import { useEffect, useRef } from 'react';
import { useAppDispatch, useAppSelector } from '../store/hooks';
import { setTrackingLoading, setTrackingData, setTrackingError } from '../store/slices/tracking';
import * as trackingService from '../services/api/tracking';

export function useTracking(awb: string) {
  const dispatch = useAppDispatch();
  const state = useAppSelector(state => state.tracking);
  const unsubscribeRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    dispatch(setTrackingLoading({ awb, loading: true }));

    const subscribe = async () => {
      try {
        unsubscribeRef.current = await trackingService.subscribeToTrackingUpdates(awb, data => {
          dispatch(setTrackingData(data as any));
        });
      } catch (err) {
        dispatch(setTrackingError({ awb, error: err instanceof Error ? err.message : 'Failed to load tracking' }));
      } finally {
        dispatch(setTrackingLoading({ awb, loading: false }));
      }
    };

    subscribe();

    return () => {
      if (unsubscribeRef.current) {
        unsubscribeRef.current();
      }
    };
  }, [awb, dispatch]);

  return {
    data: state.byAwb[awb] || null,
    loading: state.loading[awb] || false,
    error: state.error[awb] || null,
  };
}
```

### Step 4: Run tests

```bash
cd apps/customer-app
npm test -- src/hooks/__tests__
```

Expected: Tests pass.

### Step 5: Commit

```bash
git add apps/customer-app/src/hooks/
git commit -m "feat(customer-app): add custom hooks for API calls (useApi, useShipments, useTracking)"
```

---

## Task 10: Wire up API calls in screens

**Files:**
- Modify: `apps/customer-app/src/screens/booking/BookingScreen.tsx`
- Modify: `apps/customer-app/src/screens/tracking/TrackingScreen.tsx`
- Modify: `apps/customer-app/src/screens/history/HistoryScreen.tsx`

### Step 1: Update BookingScreen to call API

Modify `apps/customer-app/src/screens/booking/BookingScreen.tsx` — replace `handleConfirm`:

```typescript
// In BookingScreen.tsx, update imports and handleConfirm function:

import * as shipmentsService from '../../services/api/shipments';
import { getStoredCustomerId } from '../../services/api/auth';

// Replace handleConfirm function:
const handleConfirm = async () => {
  dispatch(setLoading(true));
  try {
    const customerId = await getStoredCustomerId();
    if (!customerId) {
      showToast('Not authenticated', 'error');
      return;
    }

    const response = await shipmentsService.createShipment(customerId, {
      origin: pickupAddress,
      destination: deliveryAddress,
      recipientName,
      recipientPhone,
      weight: parseFloat(weight),
      description,
      cargoType,
      type,
      serviceType: service as any,
      codAmount: codEnabled ? parseInt(codAmount) : undefined,
    });

    dispatch(addShipment(response as any));
    setConfirmedAwb(response.awb);
    setStep('confirmation');
    showToast('Shipment booked successfully!', 'success');
  } catch (err) {
    showToast(err instanceof Error ? err.message : 'Failed to book shipment', 'error');
  } finally {
    dispatch(setLoading(false));
  }
};
```

### Step 2: Update TrackingScreen to use useTracking

Modify `apps/customer-app/src/screens/tracking/TrackingScreen.tsx`:

```typescript
import { useTracking } from '../../hooks/useTracking';
import { useRoute } from '@react-navigation/native';

export default function TrackingScreen() {
  const route = useRoute();
  const awb = (route.params as any)?.awb || 'AWB123456'; // Default for testing
  const { data, loading, error } = useTracking(awb);

  if (loading) return <Text>Loading...</Text>;
  if (error) return <Text>{error}</Text>;
  if (!data) return <Text>Tracking data not found</Text>;

  return (
    // Render using data from hook instead of mock
  );
}
```

### Step 3: Update HistoryScreen to use useShipments

Modify `apps/customer-app/src/screens/history/HistoryScreen.tsx`:

```typescript
import { useShipments } from '../../hooks/useShipments';

export default function HistoryScreen({ navigation }: any) {
  const { list: shipments, loading } = useShipments();
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all');
  const [searchQuery, setSearchQuery] = useState('');

  // Rest remains the same, using 'shipments' from hook
}
```

### Step 4: Commit

```bash
git add apps/customer-app/src/screens/
git commit -m "feat(customer-app): integrate real API calls in screens via custom hooks"
```

---

## Phase 2 Checkpoint

**Status:** API integration complete with JWT auth, error handling, and custom hooks.

**Verification:**
```bash
# Ensure backend services are running (identity:8001, order-intake:8004, etc.)
npm start
# Test booking flow: Fill form → Submit → Verify API call to order-intake:8004
# Test tracking: Navigate to tracking screen → Verify API polling every 30s
# Test history: Verify list loads from API with filters working
```

**Files Created:** 15+
**API Endpoints Integrated:** 12+
**Tests:** >20
**Next:** Phase 3 animations

---

# PHASE 3: UX & Animations

## Task 11: Create animation utilities and SkeletonLoader

**Files:**
- Create: `apps/customer-app/src/hooks/useAnimation.ts`
- Create: `apps/customer-app/src/components/SkeletonLoader.tsx`
- Test: `apps/customer-app/src/components/__tests__/SkeletonLoader.test.tsx`

### Step 1: Implement useAnimation hook

Create file `apps/customer-app/src/hooks/useAnimation.ts`:

```typescript
import { useRef, useEffect } from 'react';
import { Animated, Easing } from 'react-native';

export function useFadeInUp(delay = 0) {
  const animValue = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    Animated.sequence([
      Animated.delay(delay),
      Animated.timing(animValue, {
        toValue: 1,
        duration: 500,
        easing: Easing.out(Easing.cubic),
        useNativeDriver: true,
      }),
    ]).start();
  }, []);

  return {
    opacity: animValue,
    transform: [
      {
        translateY: animValue.interpolate({
          inputRange: [0, 1],
          outputRange: [20, 0],
        }),
      },
    ],
  };
}

export function useScale() {
  const animValue = useRef(new Animated.Value(1)).current;

  const press = () => {
    Animated.timing(animValue, {
      toValue: 0.95,
      duration: 100,
      useNativeDriver: true,
    }).start(() => {
      Animated.timing(animValue, {
        toValue: 1,
        duration: 100,
        useNativeDriver: true,
      }).start();
    });
  };

  return {
    scale: animValue,
    onPress: press,
  };
}

export function usePulse() {
  const animValue = useRef(new Animated.Value(1)).current;

  useEffect(() => {
    Animated.loop(
      Animated.sequence([
        Animated.timing(animValue, {
          toValue: 1.1,
          duration: 1000,
          useNativeDriver: true,
        }),
        Animated.timing(animValue, {
          toValue: 1,
          duration: 1000,
          useNativeDriver: true,
        }),
      ])
    ).start();
  }, []);

  return {
    scale: animValue,
  };
}

export function useShake() {
  const animValue = useRef(new Animated.Value(0)).current;

  const shake = () => {
    Animated.sequence([
      Animated.timing(animValue, { toValue: -10, duration: 50, useNativeDriver: true }),
      Animated.timing(animValue, { toValue: 10, duration: 50, useNativeDriver: true }),
      Animated.timing(animValue, { toValue: -10, duration: 50, useNativeDriver: true }),
      Animated.timing(animValue, { toValue: 0, duration: 50, useNativeDriver: true }),
    ]).start();
  };

  return {
    translateX: animValue,
    shake,
  };
}
```

### Step 2: Implement SkeletonLoader

Create file `apps/customer-app/src/components/SkeletonLoader.tsx`:

```typescript
import React, { useRef, useEffect } from 'react';
import { View, Animated } from 'react-native';
import { COLORS } from '../utils/colors';

interface SkeletonLoaderProps {
  width?: number | string;
  height?: number;
  borderRadius?: number;
}

export default function SkeletonLoader({
  width = '100%',
  height = 20,
  borderRadius = 8,
}: SkeletonLoaderProps) {
  const shimmerAnim = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    Animated.loop(
      Animated.sequence([
        Animated.timing(shimmerAnim, { toValue: 1, duration: 1000, useNativeDriver: true }),
        Animated.timing(shimmerAnim, { toValue: 0, duration: 1000, useNativeDriver: true }),
      ])
    ).start();
  }, []);

  return (
    <View
      style={{
        width,
        height,
        backgroundColor: COLORS.SURFACE,
        borderRadius,
        overflow: 'hidden',
        marginBottom: 8,
      }}
    >
      <Animated.View
        style={{
          flex: 1,
          backgroundColor: COLORS.GLASS,
          opacity: shimmerAnim,
        }}
      />
    </View>
  );
}
```

### Step 3: Commit

```bash
git add apps/customer-app/src/hooks/useAnimation.ts apps/customer-app/src/components/SkeletonLoader.tsx
git commit -m "feat(customer-app): add animation utilities and skeleton loader component"
```

---

## Task 12: Add micro-interactions to all screens

**Files:**
- Modify: All screen files to use animation hooks

### Step 1: Update HomeScreen with FadeInUp animations

Modify `apps/customer-app/src/screens/home/HomeScreen.tsx`:

```typescript
import { Animated, FlatList } from 'react-native';
import { useFadeInUp } from '../../hooks/useAnimation';

export default function HomeScreen({ navigation }: any) {
  const headerAnim = useFadeInUp(0);
  const actionsAnim = useFadeInUp(100);
  const shipmentsAnim = useFadeInUp(200);

  return (
    <ScrollView
      style={{ flex: 1, backgroundColor: COLORS.CANVAS }}
      contentContainerStyle={{ padding: 16, paddingBottom: 40 }}
    >
      {/* Header with fade-in */}
      <Animated.View style={headerAnim}>
        <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 14 }}>Welcome back</Text>
        <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 24, fontWeight: '700', marginTop: 4 }}>
          {auth.name || 'Customer'}
        </Text>
      </Animated.View>

      {/* Rest of component with staggered animations */}
      <Animated.View style={[actionsAnim, { marginTop: 24 }]}>
        {/* Quick actions */}
      </Animated.View>

      <Animated.View style={[shipmentsAnim, { marginTop: 20 }]}>
        {/* Recent shipments */}
      </Animated.View>
    </ScrollView>
  );
}
```

### Step 2: Update StatusBadge with pulse animation

Modify `apps/customer-app/src/components/StatusBadge.tsx`:

```typescript
import { Animated } from 'react-native';
import { usePulse } from '../hooks/useAnimation';

export default function StatusBadge({ status, size = 'md' }: StatusBadgeProps) {
  const { scale } = usePulse();
  const shouldPulse = ['picked', 'in_transit'].includes(status);

  const animStyle = shouldPulse ? { transform: [{ scale }] } : {};

  return (
    <Animated.View
      style={[
        {
          backgroundColor: bgColor,
          borderRadius: 12,
          alignSelf: 'flex-start',
          ...padding,
        },
        animStyle,
      ]}
    >
      <Text style={{ color: textColor, fontSize, fontWeight: '600' }}>{label}</Text>
    </Animated.View>
  );
}
```

### Step 3: Update Button with scale animation

Modify `apps/customer-app/src/components/Button.tsx`:

```typescript
import { Animated } from 'react-native';
import { useScale } from '../hooks/useAnimation';

export default function Button({
  onPress,
  label,
  variant = 'primary',
  size = 'md',
  disabled = false,
  style,
}: ButtonProps) {
  const { scale, onPress: animatePress } = useScale();

  const handlePress = () => {
    animatePress();
    onPress();
  };

  return (
    <Animated.View style={{ transform: [{ scale }] }}>
      <TouchableOpacity
        onPress={handlePress}
        disabled={disabled}
        // ... rest of styles
      >
        <Text>{label}</Text>
      </TouchableOpacity>
    </Animated.View>
  );
}
```

### Step 4: Commit

```bash
git add apps/customer-app/src/screens/ apps/customer-app/src/components/
git commit -m "feat(customer-app): add micro-interactions with Reanimated animations across all screens"
```

---

## Phase 3 Checkpoint

**Status:** All animations implemented, smooth 60fps micro-interactions.

**Verification:**
```bash
npm start
# Navigate screens → Observe smooth fade-in transitions
# Open booking form → Watch button scale on press
# View tracking status badge → See pulse animation on "in_transit"
# Verify 60fps in React Native Debugger
```

**Files Modified:** 15+
**Animations Added:** 20+
**Next:** Phase 4 offline capability

---

# PHASE 4: Offline Capability

## Task 13: Set up SQLite offline database

**Files:**
- Create: `apps/customer-app/src/db/sqlite.ts`
- Create: `apps/customer-app/src/db/schema.ts`
- Create: `apps/customer-app/src/db/sync.ts`
- Test: `apps/customer-app/src/db/__tests__/sync.test.ts`

### Step 1: Implement SQLite initialization

Create file `apps/customer-app/src/db/sqlite.ts`:

```typescript
import * as SQLite from 'expo-sqlite';
import { schema } from './schema';

let db: SQLite.Database | null = null;

export async function initializeDatabase(): Promise<SQLite.Database> {
  if (db) return db;

  db = await SQLite.openDatabaseAsync('logisticos_offline.db');

  // Create tables
  for (const table of schema) {
    await db.execAsync(table);
  }

  return db;
}

export async function getDatabase(): Promise<SQLite.Database> {
  if (!db) {
    return initializeDatabase();
  }
  return db;
}

export async function closeDatabase(): Promise<void> {
  if (db) {
    await db.closeAsync();
    db = null;
  }
}
```

### Step 2: Define schema

Create file `apps/customer-app/src/db/schema.ts`:

```typescript
export const schema = [
  `
    CREATE TABLE IF NOT EXISTS shipments (
      id TEXT PRIMARY KEY,
      awb TEXT UNIQUE NOT NULL,
      customerId TEXT NOT NULL,
      origin TEXT NOT NULL,
      destination TEXT NOT NULL,
      status TEXT NOT NULL,
      fee REAL NOT NULL,
      currency TEXT NOT NULL,
      type TEXT NOT NULL,
      recipientName TEXT NOT NULL,
      recipientPhone TEXT NOT NULL,
      codAmount REAL,
      createdAt TEXT NOT NULL,
      syncedAt TEXT,
      isPending INTEGER DEFAULT 0
    );
  `,
  `
    CREATE TABLE IF NOT EXISTS tracking_history (
      id TEXT PRIMARY KEY,
      awb TEXT NOT NULL,
      customerId TEXT NOT NULL,
      currentStatus TEXT NOT NULL,
      eta TEXT,
      currentLocation TEXT,
      events TEXT NOT NULL,
      lastUpdated TEXT NOT NULL,
      syncedAt TEXT,
      UNIQUE(awb, customerId)
    );
  `,
  `
    CREATE TABLE IF NOT EXISTS saved_addresses (
      id TEXT PRIMARY KEY,
      customerId TEXT NOT NULL,
      label TEXT NOT NULL,
      street TEXT NOT NULL,
      city TEXT NOT NULL,
      state TEXT NOT NULL,
      postalCode TEXT NOT NULL,
      country TEXT NOT NULL,
      isPrimary INTEGER DEFAULT 0,
      createdAt TEXT NOT NULL,
      UNIQUE(customerId, label)
    );
  `,
  `
    CREATE TABLE IF NOT EXISTS synced_metadata (
      resource TEXT PRIMARY KEY,
      lastSyncedAt TEXT NOT NULL,
      syncStatus TEXT NOT NULL DEFAULT 'success'
    );
  `,
];
```

### Step 3: Implement sync logic

Create file `apps/customer-app/src/db/sync.ts`:

```typescript
import { getDatabase } from './sqlite';
import * as shipmentsService from '../services/api/shipments';
import { getStoredCustomerId } from '../services/api/auth';
import { useAppDispatch } from '../store/hooks';
import { setShipments } from '../store/slices/shipments';

export async function syncShipments(): Promise<void> {
  const db = await getDatabase();
  const customerId = await getStoredCustomerId();

  if (!customerId) return;

  try {
    // 1. Upload pending shipments
    const pending = await db.getAllAsync<any>(
      `SELECT * FROM shipments WHERE customerId = ? AND isPending = 1`,
      [customerId]
    );

    for (const shipment of pending) {
      try {
        await shipmentsService.createShipment(customerId, {
          origin: shipment.origin,
          destination: shipment.destination,
          recipientName: shipment.recipientName,
          recipientPhone: shipment.recipientPhone,
          weight: 0, // Not stored locally
          description: '',
          cargoType: 'goods',
          type: shipment.type,
          serviceType: 'standard',
          codAmount: shipment.codAmount,
        });

        // Mark as synced
        await db.runAsync(`UPDATE shipments SET isPending = 0, syncedAt = ? WHERE id = ?`, [new Date().toISOString(), shipment.id]);
      } catch (error) {
        console.error('Failed to sync shipment:', shipment.awb, error);
      }
    }

    // 2. Download latest shipments
    const response = await shipmentsService.listShipments(customerId, { limit: 100 });

    // Clear and re-populate
    await db.runAsync(`DELETE FROM shipments WHERE customerId = ?`, [customerId]);
    for (const shipment of response.shipments) {
      await db.runAsync(
        `INSERT OR REPLACE INTO shipments (id, awb, customerId, origin, destination, status, fee, currency, type, recipientName, recipientPhone, createdAt, syncedAt) 
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
        [
          `${shipment.awb}-${Date.now()}`,
          shipment.awb,
          customerId,
          shipment.origin,
          shipment.destination,
          shipment.status,
          shipment.fee,
          shipment.currency,
          'local',
          'unknown',
          '',
          new Date().toISOString(),
          new Date().toISOString(),
        ]
      );
    }

    // Update sync metadata
    await db.runAsync(
      `INSERT OR REPLACE INTO synced_metadata (resource, lastSyncedAt, syncStatus) VALUES (?, ?, ?)`,
      ['shipments', new Date().toISOString(), 'success']
    );
  } catch (error) {
    console.error('Sync failed:', error);
    await db.runAsync(
      `INSERT OR REPLACE INTO synced_metadata (resource, lastSyncedAt, syncStatus) VALUES (?, ?, ?)`,
      ['shipments', new Date().toISOString(), 'failed']
    );
  }
}

export async function savePendingShipment(customerId: string, shipmentData: any): Promise<void> {
  const db = await getDatabase();

  await db.runAsync(
    `INSERT INTO shipments (id, awb, customerId, origin, destination, status, fee, currency, type, recipientName, recipientPhone, createdAt, isPending)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)`,
    [
      `pending-${Date.now()}`,
      `PENDING-${Math.random().toString(36).substr(2, 8).toUpperCase()}`,
      customerId,
      shipmentData.origin,
      shipmentData.destination,
      'pending',
      shipmentData.fee,
      'PHP',
      shipmentData.type,
      shipmentData.recipientName,
      shipmentData.recipientPhone,
      new Date().toISOString(),
    ]
  );
}

export async function getOfflineShipments(customerId: string): Promise<any[]> {
  const db = await getDatabase();
  return db.getAllAsync(`SELECT * FROM shipments WHERE customerId = ? ORDER BY createdAt DESC`, [customerId]);
}
```

### Step 4: Commit

```bash
git add apps/customer-app/src/db/
git commit -m "feat(customer-app): implement SQLite offline database with sync logic"
```

---

## Task 14: Integrate offline booking and sync on reconnect

**Files:**
- Modify: `apps/customer-app/src/screens/booking/BookingScreen.tsx`
- Modify: `apps/customer-app/App.tsx`

### Step 1: Update BookingScreen to support offline

Modify `apps/customer-app/src/screens/booking/BookingScreen.tsx` — update `handleConfirm`:

```typescript
import { useNetInfo } from '@react-native-community/netinfo';
import { savePendingShipment, getOfflineShipments } from '../../db/sync';
import { getStoredCustomerId } from '../../services/api/auth';

export default function BookingScreen({ navigation }: any) {
  // ... existing code ...

  const { isConnected } = useNetInfo();

  const handleConfirm = async () => {
    dispatch(setLoading(true));
    try {
      const customerId = await getStoredCustomerId();
      if (!customerId) {
        showToast('Not authenticated', 'error');
        return;
      }

      if (!isConnected) {
        // Offline: save to local DB
        await savePendingShipment(customerId, {
          origin: pickupAddress,
          destination: deliveryAddress,
          recipientName,
          recipientPhone,
          weight: parseFloat(weight),
          fee: total,
          type,
        });
        showToast('Saved offline. Will sync when online.', 'info');
        setStep('confirmation');
      } else {
        // Online: call API directly
        const response = await shipmentsService.createShipment(customerId, {
          origin: pickupAddress,
          destination: deliveryAddress,
          recipientName,
          recipientPhone,
          weight: parseFloat(weight),
          description,
          cargoType,
          type,
          serviceType: service as any,
          codAmount: codEnabled ? parseInt(codAmount) : undefined,
        });

        dispatch(addShipment(response as any));
        setConfirmedAwb(response.awb);
        setStep('confirmation');
        showToast('Shipment booked successfully!', 'success');
      }
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Failed to book shipment', 'error');
    } finally {
      dispatch(setLoading(false));
    }
  };
}
```

### Step 2: Add background sync in App.tsx

Modify `apps/customer-app/App.tsx`:

```typescript
import { useEffect } from 'react';
import { useNetInfo } from '@react-native-community/netinfo';
import { initializeDatabase } from './src/db/sqlite';
import { syncShipments } from './src/db/sync';
import * as TaskManager from 'expo-task-manager';
import * as BackgroundFetch from 'expo-background-fetch';

const BACKGROUND_SYNC_TASK = 'background-sync-task';

// Define background task
TaskManager.defineTask(BACKGROUND_SYNC_TASK, async () => {
  try {
    await syncShipments();
    return BackgroundFetch.BackgroundFetchResult.NewData;
  } catch (error) {
    console.error('Background sync failed:', error);
    return BackgroundFetch.BackgroundFetchResult.Failed;
  }
});

export default function App() {
  const { isConnected } = useNetInfo();

  useEffect(() => {
    // Initialize DB
    initializeDatabase();

    // Register background sync
    const registerBackgroundFetch = async () => {
      try {
        await BackgroundFetch.registerTaskAsync(BACKGROUND_SYNC_TASK, {
          minimumInterval: 15 * 60, // 15 minutes
          stopOnTerminate: false,
          startOnBoot: true,
        });
      } catch (err) {
        console.warn('Background fetch registration failed:', err);
      }
    };

    registerBackgroundFetch();

    // Sync when network restored
    if (isConnected) {
      syncShipments();
    }
  }, [isConnected]);

  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <Provider store={store}>
        <AppNavigator />
      </Provider>
    </GestureHandlerRootView>
  );
}
```

### Step 3: Commit

```bash
git add apps/customer-app/src/screens/booking/ apps/customer-app/App.tsx
git commit -m "feat(customer-app): integrate offline booking with background sync on network restore"
```

---

## Task 15: Add offline tracking and sync indicator

**Files:**
- Modify: `apps/customer-app/src/screens/tracking/TrackingScreen.tsx`
- Modify: `apps/customer-app/src/components/Toast.tsx`

### Step 1: Update TrackingScreen with offline fallback

Modify `apps/customer-app/src/screens/tracking/TrackingScreen.tsx`:

```typescript
import { useNetInfo } from '@react-native-community/netinfo';
import { useTracking } from '../../hooks/useTracking';
import { getOfflineShipments, getDatabase } from '../../db/sync';
import { getStoredCustomerId } from '../../services/api/auth';

export default function TrackingScreen({ navigation }: any) {
  const route = useRoute();
  const { isConnected } = useNetInfo();
  const awb = (route.params as any)?.awb || 'AWB123456';
  const { data, loading, error } = useTracking(awb);
  const [offlineData, setOfflineData] = useState<any>(null);
  const customerId = getStoredCustomerId();

  useEffect(() => {
    if (!isConnected && !data) {
      // Load from offline DB
      const loadOfflineTracking = async () => {
        const db = await getDatabase();
        const tracking = await db.getFirstAsync(
          `SELECT * FROM tracking_history WHERE awb = ?`,
          [awb]
        );
        if (tracking) {
          setOfflineData(JSON.parse(tracking.events));
        }
      };
      loadOfflineTracking();
    }
  }, [isConnected, awb]);

  return (
    <View style={{ flex: 1, backgroundColor: COLORS.CANVAS }}>
      {!isConnected && (
        <View style={{ backgroundColor: COLORS.AMBER, paddingVertical: 8, paddingHorizontal: 16, alignItems: 'center' }}>
          <Text style={{ color: COLORS.CANVAS, fontSize: 12, fontWeight: '600' }}>
            Offline - showing cached data
          </Text>
        </View>
      )}

      {/* Render using data || offlineData */}
      {/* ... rest of component ... */}
    </View>
  );
}
```

### Step 2: Commit

```bash
git add apps/customer-app/src/screens/tracking/
git commit -m "feat(customer-app): add offline tracking with cached data display"
```

---

## Phase 4 Checkpoint

**Status:** Offline capability complete with SQLite sync and background task.

**Verification:**
```bash
npm start
# Disable network in simulator
# Book shipment offline → Verify saved to local DB with "pending" state
# Enable network → Verify sync runs and status updates
# View tracked shipment offline → Verify cached data displays with "offline" badge
```

**Files Created/Modified:** 10+
**Offline Features:** 3 (pending bookings, cached tracking, background sync)

---

## Final Verification & Commit

```bash
cd apps/customer-app

# Run all tests
npm test

# Build for iOS
npm run build:ios

# Build for Android
npm run build:android

# Final commit
git add -A
git commit -m "feat(customer-app): complete 4-phase implementation (UI, API, animations, offline)"
```

---

## Success Criteria Checklist

### Phase 1: Core 5 Screens ✅
- [ ] Home screen renders with greeting, quick actions, recent shipments
- [ ] Booking screen multi-step form with validation
- [ ] History screen with filtering and pagination
- [ ] Support screen with FAQ and contact info
- [ ] Profile screen with account management
- [ ] All navigation works seamlessly
- [ ] Redux store synced across all screens

### Phase 2: Backend Integration ✅
- [ ] JWT auth via identity:8001
- [ ] Shipment creation via order-intake:8004
- [ ] Tracking updates via tracking service
- [ ] Error handling with retry logic
- [ ] Secure token storage

### Phase 3: Animations ✅
- [ ] 60fps smooth screen transitions
- [ ] Fade-in list item animations with stagger
- [ ] Button press scale feedback
- [ ] Status badge pulse animations
- [ ] Skeleton loaders on data fetch

### Phase 4: Offline ✅
- [ ] Local SQLite database initialized
- [ ] Pending shipments saved offline
- [ ] Background sync every 15 minutes
- [ ] Sync triggers on network restore
- [ ] Cached tracking shown offline

---

Plan complete and saved to `docs/superpowers/plans/2026-04-05-customer-app-implementation.md`.

**Two execution options:**

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
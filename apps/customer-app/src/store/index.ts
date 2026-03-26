/**
 * Customer App — Redux Store
 */
import { configureStore, createSlice, PayloadAction } from "@reduxjs/toolkit";

// ── Auth slice ─────────────────────────────────────────────────────────────────

export type KycStatus       = "none" | "pending" | "verified" | "rejected";
export type IdType          = "passport" | "emirates_id";
export type VerificationTier = "guest" | "phone_verified" | "id_verified";

interface AuthState {
  token:             string | null;
  customerId:        string | null;
  name:              string | null;
  phone:             string | null;
  email:             string | null;
  loyaltyPts:        number;
  isGuest:           boolean;
  kycStatus:         KycStatus;
  idType:            IdType | null;
  verificationTier:  VerificationTier;
  onboardingStep:    "phone" | "profile" | "kyc" | "complete";
}

const authSlice = createSlice({
  name: "auth",
  initialState: {
    token: null, customerId: null, name: null, phone: null, email: null,
    loyaltyPts: 0, isGuest: true,
    kycStatus: "none", idType: null,
    verificationTier: "guest",
    onboardingStep: "phone",
  } as AuthState,
  reducers: {
    setCredentials: (state, action: PayloadAction<Omit<AuthState, "isGuest" | "kycStatus" | "idType" | "verificationTier" | "onboardingStep">>) => {
      Object.assign(state, action.payload, { isGuest: false });
    },
    setPhone: (state, action: PayloadAction<string>) => {
      state.phone = action.payload;
      state.verificationTier = "phone_verified";
      state.isGuest = false;
      state.onboardingStep = "profile";
    },
    setProfile: (state, action: PayloadAction<{ name: string; email?: string; customerId: string }>) => {
      state.name       = action.payload.name;
      state.email      = action.payload.email ?? null;
      state.customerId = action.payload.customerId;
      state.onboardingStep = "kyc";
    },
    submitKyc: (state, action: PayloadAction<{ idType: IdType }>) => {
      state.idType     = action.payload.idType;
      state.kycStatus  = "pending";
      state.onboardingStep = "complete";
    },
    approveKyc: (state) => {
      state.kycStatus         = "verified";
      state.verificationTier  = "id_verified";
    },
    addLoyaltyPts: (state, action: PayloadAction<number>) => {
      state.loyaltyPts += action.payload;
    },
    logout: (state) => {
      state.token = null; state.customerId = null; state.name = null;
      state.phone = null; state.email = null; state.loyaltyPts = 0;
      state.isGuest = true; state.kycStatus = "none"; state.idType = null;
      state.verificationTier = "guest"; state.onboardingStep = "phone";
    },
  },
});

// ── Tracking slice ─────────────────────────────────────────────────────────────

export interface TrackingHistoryItem {
  tracking_number: string;
  status:          string;
  searched_at:     string;
}

interface TrackingState {
  history: TrackingHistoryItem[];
}

const trackingSlice = createSlice({
  name: "tracking",
  initialState: { history: [] } as TrackingState,
  reducers: {
    addToHistory: (state, action: PayloadAction<TrackingHistoryItem>) => {
      const existing = state.history.findIndex(h => h.tracking_number === action.payload.tracking_number);
      if (existing >= 0) {
        state.history[existing] = action.payload;
      } else {
        state.history.unshift(action.payload);
        if (state.history.length > 20) state.history.pop();
      }
    },
    clearHistory: (state) => {
      state.history = [];
    },
  },
});

// ── Shipments slice ────────────────────────────────────────────────────────────

export type ShipmentStatus =
  | "pending" | "confirmed" | "picked_up"
  | "in_transit" | "out_for_delivery"
  | "delivery_attempted" | "delivered" | "returned" | "cancelled";

export type ShipmentType = "local" | "international";

export interface ShipmentRecord {
  awb:          string;
  type:         ShipmentType;
  status:       ShipmentStatus;
  origin:       string;
  destination:  string;
  destCountry?: string;
  description:  string;
  weight?:      string;
  isCOD:        boolean;
  codAmount?:   string;
  freightMode?: "sea" | "air";
  bookedAt:     string;
  estimatedDelivery?: string;
  totalFee:     number;
}

interface ShipmentsState {
  list: ShipmentRecord[];
}

const shipmentsSlice = createSlice({
  name: "shipments",
  initialState: { list: [] } as ShipmentsState,
  reducers: {
    addShipment: (state, action: PayloadAction<ShipmentRecord>) => {
      state.list.unshift(action.payload);
    },
    updateShipmentStatus: (state, action: PayloadAction<{ awb: string; status: ShipmentStatus }>) => {
      const found = state.list.find(s => s.awb === action.payload.awb);
      if (found) found.status = action.payload.status;
    },
  },
});

// ── Preferences slice ──────────────────────────────────────────────────────────

interface PrefsState {
  notifDelivery: boolean;
  notifPromos:   boolean;
}

const prefsSlice = createSlice({
  name: "prefs",
  initialState: { notifDelivery: true, notifPromos: true } as PrefsState,
  reducers: {
    setNotifDelivery: (state, action: PayloadAction<boolean>) => { state.notifDelivery = action.payload; },
    setNotifPromos:   (state, action: PayloadAction<boolean>) => { state.notifPromos   = action.payload; },
  },
});

// ── Store ──────────────────────────────────────────────────────────────────────

export const store = configureStore({
  reducer: {
    auth:      authSlice.reducer,
    tracking:  trackingSlice.reducer,
    shipments: shipmentsSlice.reducer,
    prefs:     prefsSlice.reducer,
  },
});

export type RootState   = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;

export const authActions      = authSlice.actions;
export const trackingActions  = trackingSlice.actions;
export const shipmentsActions = shipmentsSlice.actions;
export const prefsActions     = prefsSlice.actions;
export type  { AuthState };

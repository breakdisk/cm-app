/**
 * Redux store — driver app state management.
 */
import { configureStore, createSlice, PayloadAction } from "@reduxjs/toolkit";

// ── Auth slice ────────────────────────────────────────────────────────────────

interface AuthState {
  token:     string | null;
  driverId:  string | null;
  name:      string | null;
  isOnline:  boolean;
}

const authSlice = createSlice({
  name:         "auth",
  initialState: { token: null, driverId: null, name: null, isOnline: false } as AuthState,
  reducers: {
    setCredentials: (state, action: PayloadAction<{ token: string; driverId: string; name: string }>) => {
      state.token    = action.payload.token;
      state.driverId = action.payload.driverId;
      state.name     = action.payload.name;
    },
    setOnlineStatus: (state, action: PayloadAction<boolean>) => {
      state.isOnline = action.payload;
    },
    logout: (state) => {
      state.token    = null;
      state.driverId = null;
      state.name     = null;
      state.isOnline = false;
    },
  },
});

// ── Task slice ────────────────────────────────────────────────────────────────

export type TaskStatus =
  | "assigned"
  | "navigating"
  | "arrived"
  | "pod_pending"
  | "completed"
  | "failed"
  | "awaiting_pickup"   // first-mile: driver en route to sender
  | "pickup_confirmed"; // first-mile: package in hand

export type TaskType = "delivery" | "pickup";

export interface DeliveryTask {
  id:              string;
  shipment_id:     string;
  tracking_number: string;
  sequence:        number;
  status:          TaskStatus;
  task_type:       TaskType;
  // Delivery fields
  recipient_name:  string;
  recipient_phone: string;
  address_line1:   string;
  address_city:    string;
  lat:             number;
  lng:             number;
  cod_amount?:     number;  // in PHP, null if prepaid
  special_notes?:  string;
  attempt_count:   number;
  eta_minutes?:    number;
  // Pickup-specific fields
  sender_name?:    string;
  sender_phone?:   string;
  package_desc?:   string;
  package_weight?: string;
}

interface TaskState {
  tasks:         DeliveryTask[];
  selectedId:    string | null;
  syncPending:   number;  // count of unsynced local actions
}

const taskSlice = createSlice({
  name: "tasks",
  initialState: {
    tasks: [] as DeliveryTask[],
    selectedId: null,
    syncPending: 0,
  } as TaskState,
  reducers: {
    setTasks: (state, action: PayloadAction<DeliveryTask[]>) => {
      state.tasks = action.payload;
    },
    updateTaskStatus: (state, action: PayloadAction<{ id: string; status: TaskStatus }>) => {
      const task = state.tasks.find((t) => t.id === action.payload.id);
      if (task) task.status = action.payload.status;
    },
    setSelected: (state, action: PayloadAction<string | null>) => {
      state.selectedId = action.payload;
    },
    incrementSyncPending: (state) => { state.syncPending += 1; },
    decrementSyncPending: (state) => { state.syncPending = Math.max(0, state.syncPending - 1); },
  },
});

// ── Earnings slice ────────────────────────────────────────────────────────────

export type DriverType = "full_time" | "part_time";

export interface EarningEntry {
  taskId:      string;
  shipmentId:  string;
  completedAt: string;  // ISO timestamp
  baseAmount:  number;  // base commission in PHP
  codBonus:    number;  // COD commission in PHP (0 if prepaid)
  total:       number;
}

interface EarningsState {
  driverType:          DriverType;
  commissionRate:      number;  // PHP per delivery (part-time only)
  codCommissionRate:   number;  // % of COD amount as bonus (e.g. 0.02 = 2%)
  todayEarnings:       number;
  weekEarnings:        number;
  pendingPayout:       number;
  breakdown:           EarningEntry[];
}

const earningsSlice = createSlice({
  name: "earnings",
  initialState: {
    driverType:        "full_time",
    commissionRate:    0,
    codCommissionRate: 0,
    todayEarnings:     0,
    weekEarnings:      0,
    pendingPayout:     0,
    breakdown:         [],
  } as EarningsState,
  reducers: {
    setDriverConfig: (
      state,
      action: PayloadAction<{
        driverType:        DriverType;
        commissionRate:    number;
        codCommissionRate: number;
      }>
    ) => {
      state.driverType        = action.payload.driverType;
      state.commissionRate    = action.payload.commissionRate;
      state.codCommissionRate = action.payload.codCommissionRate;
    },
    recordDeliveryEarning: (state, action: PayloadAction<EarningEntry>) => {
      state.breakdown.push(action.payload);
      state.todayEarnings  += action.payload.total;
      state.weekEarnings   += action.payload.total;
      state.pendingPayout  += action.payload.total;
    },
    confirmPayout: (state, action: PayloadAction<number>) => {
      state.pendingPayout = Math.max(0, state.pendingPayout - action.payload);
    },
    resetDailyEarnings: (state) => {
      state.todayEarnings = 0;
      state.breakdown     = state.breakdown.filter((e) => {
        const today = new Date().toISOString().slice(0, 10);
        return e.completedAt.slice(0, 10) !== today;
      });
    },
  },
});

// ── Compliance slice ──────────────────────────────────────────────────────────

export interface RequiredDocType {
  id:               string;
  code:             string;
  name:             string;
  has_expiry:       boolean;
  warn_days_before: number;
}

export interface SubmittedDoc {
  id:               string;
  document_type_id: string;
  document_number:  string;
  expiry_date:      string | null;
  status:           "submitted" | "under_review" | "approved" | "rejected" | "expired" | "superseded";
  rejection_reason: string | null;
  submitted_at:     string;
}

export interface ComplianceState {
  overall_status: string;  // "pending_submission" | "under_review" | "compliant" | "expiring_soon" | "expired" | "suspended"
  jurisdiction:   string;
  required_types: RequiredDocType[];
  documents:      SubmittedDoc[];
}

const initialComplianceState: ComplianceState = {
  overall_status: "pending_submission",
  jurisdiction:   "UAE",
  required_types: [],
  documents:      [],
};

const complianceSlice = createSlice({
  name:         "compliance",
  initialState: initialComplianceState,
  reducers: {
    setComplianceProfile(
      state,
      action: PayloadAction<{
        overall_status: string;
        jurisdiction:   string;
        required_types: RequiredDocType[];
        documents:      SubmittedDoc[];
      }>
    ) {
      state.overall_status = action.payload.overall_status;
      state.jurisdiction   = action.payload.jurisdiction;
      state.required_types = action.payload.required_types;
      state.documents      = action.payload.documents;
    },
    upsertDocument(state, action: PayloadAction<SubmittedDoc>) {
      const idx = state.documents.findIndex((d) => d.id === action.payload.id);
      if (idx >= 0) {
        state.documents[idx] = action.payload;
      } else {
        state.documents.push(action.payload);
      }
    },
  },
});

// ── Store ─────────────────────────────────────────────────────────────────────

export const store = configureStore({
  reducer: {
    auth:       authSlice.reducer,
    tasks:      taskSlice.reducer,
    earnings:   earningsSlice.reducer,
    compliance: complianceSlice.reducer,
  },
});

export type RootState   = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;

export const authActions       = authSlice.actions;
export const taskActions       = taskSlice.actions;
export const earningsActions   = earningsSlice.actions;
export const complianceActions = complianceSlice.actions;

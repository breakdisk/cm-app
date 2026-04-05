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
  currentStatus?: string;
  status?: string;
  tracking_number?: string;
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
  history?: TrackingInfo[];
}

const initialState: TrackingState = {
  byAwb: {},
  loading: {},
  error: {},
  lastUpdated: {},
  history: [],
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
    addToHistory: (state, action: PayloadAction<TrackingInfo>) => {
      if (!state.history) state.history = [];
      state.history.push(action.payload);
    },
  },
});

export const { setTrackingLoading, setTrackingData, setTrackingError, addToHistory } = trackingSlice.actions;

export const trackingActions = {
  setTrackingLoading,
  setTrackingData,
  setTrackingError,
  addToHistory,
};

export default trackingSlice.reducer;

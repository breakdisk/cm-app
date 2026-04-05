import { createSlice, PayloadAction } from '@reduxjs/toolkit';

export type ShipmentStatusType = 'pending' | 'processing' | 'picked' | 'in_transit' | 'delivered' | 'failed' | 'cancelled' | 'confirmed' | 'picked_up' | 'out_for_delivery' | 'delivery_attempted' | 'returned';

export interface Shipment {
  awb: string;
  status: ShipmentStatusType;
  origin: string;
  destination: string;
  date?: string;
  fee?: number;
  totalFee?: number;
  currency?: 'PHP' | 'USD';
  codAmount?: string | number;
  isCOD?: boolean;
  type: 'local' | 'international';
  recipientName?: string;
  recipientPhone?: string;
  description?: string;
  weight?: string | number;
  bookedAt?: string;
  estimatedDelivery?: string;
  destCountry?: string;
  freightMode?: 'sea' | 'air';
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

export type ShipmentRecord = Shipment;
export type ShipmentStatus = Shipment['status'];

export const shipmentsActions = {
  setLoading,
  setShipments,
  addShipment,
  updateShipment,
  setError,
  setPagination,
};

export default shipmentsSlice.reducer;

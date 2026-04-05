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

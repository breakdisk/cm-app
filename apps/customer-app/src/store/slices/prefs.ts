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

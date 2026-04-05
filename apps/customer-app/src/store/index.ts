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

import { configureStore } from '@reduxjs/toolkit';
import authReducer, * as authActions from './slices/auth';
import shipmentsReducer, * as shipmentsActions from './slices/shipments';
import trackingReducer, * as trackingActions from './slices/tracking';
import prefsReducer, * as prefsActions from './slices/prefs';
import addressesReducer from './slices/addresses';
import invoicesReducer, * as invoicesActions from './slices/invoices';

export const store = configureStore({
  reducer: {
    auth:      authReducer,
    shipments: shipmentsReducer,
    tracking:  trackingReducer,
    prefs:     prefsReducer,
    addresses: addressesReducer,
    invoices:  invoicesReducer,
  },
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;

// Export slices and actions
export { authActions, shipmentsActions, trackingActions, prefsActions, invoicesActions };
export * from './slices/auth';
export * from './slices/shipments';
export * from './slices/tracking';
export * from './slices/prefs';
// invoices slice — use named imports from the slice directly to avoid action name collisions
export type { InvoicesState, InvoiceSummary, InvoiceDetail } from './slices/invoices';
export { default as invoicesReducer } from './slices/invoices';

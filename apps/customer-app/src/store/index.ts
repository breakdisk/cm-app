import { configureStore } from '@reduxjs/toolkit';
import authReducer, * as authActions from './slices/auth';
// Reducer only. These two slices export their own `shipmentsActions` /
// `trackingActions` objects, and `export * from './slices/…'` below already
// re-exports those — so aliasing the whole module under the same name created
// two different things called `shipmentsActions`: the module namespace, and
// the slice's action object nested inside it. Consumers got whichever the
// explicit export list won with, and `shipmentsActions.shipmentsActions` was
// also valid. Both resolve the same action creators, so this changes no
// behaviour; it removes the ambiguity ESLint's import/export flagged.
import shipmentsReducer from './slices/shipments';
import trackingReducer from './slices/tracking';
import prefsReducer, * as prefsActions from './slices/prefs';
import addressesReducer from './slices/addresses';
import invoicesReducer, * as invoicesActions from './slices/invoices';
import brandingReducer from './slices/branding';

export const store = configureStore({
  reducer: {
    auth:      authReducer,
    shipments: shipmentsReducer,
    tracking:  trackingReducer,
    prefs:     prefsReducer,
    addresses: addressesReducer,
    invoices:  invoicesReducer,
    branding:  brandingReducer,
  },
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;

// Export slices and actions
// shipmentsActions / trackingActions are not listed here: they come from the
// `export *` lines below, which re-export the slices' own action objects.
export { authActions, prefsActions, invoicesActions };
export * from './slices/auth';
export * from './slices/shipments';
export * from './slices/tracking';
export * from './slices/prefs';
// invoices slice — use named imports from the slice directly to avoid action name collisions
export type { InvoicesState, InvoiceSummary, InvoiceDetail } from './slices/invoices';
export { default as invoicesReducer } from './slices/invoices';
export { hydrateBrandingFromCache, fetchBranding, resetBranding } from './slices/branding';
export type { BrandingState } from './slices/branding';

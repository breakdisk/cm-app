import { createSlice, PayloadAction } from '@reduxjs/toolkit';
import type { InvoiceSummary, InvoiceDetail } from '../../services/api/invoices';

export type { InvoiceSummary, InvoiceDetail };

export interface InvoicesState {
  list:     InvoiceSummary[];
  byId:     Record<string, InvoiceDetail>;
  loading:  boolean;
  error:    string | null;
}

const initialState: InvoicesState = {
  list:    [],
  byId:    {},
  loading: false,
  error:   null,
};

const invoicesSlice = createSlice({
  name: 'invoices',
  initialState,
  reducers: {
    setLoading: (state, action: PayloadAction<boolean>) => {
      state.loading = action.payload;
    },
    setError: (state, action: PayloadAction<string | null>) => {
      state.error   = action.payload;
      state.loading = false;
    },
    setList: (state, action: PayloadAction<InvoiceSummary[]>) => {
      state.list    = action.payload;
      state.loading = false;
      state.error   = null;
    },
    setDetail: (state, action: PayloadAction<InvoiceDetail>) => {
      state.byId[action.payload.id] = action.payload;
      state.loading = false;
      state.error   = null;
    },
    clearInvoices: state => {
      state.list    = [];
      state.byId    = {};
      state.error   = null;
    },
  },
});

export const {
  setLoading,
  setError,
  setList,
  setDetail,
  clearInvoices,
} = invoicesSlice.actions;

export default invoicesSlice.reducer;

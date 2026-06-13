import { createSlice, createAsyncThunk, PayloadAction } from '@reduxjs/toolkit';
import * as SecureStore from 'expo-secure-store';
import { fetchBrandingApi, BrandingDto } from '../../services/api/branding';

const CACHE_KEY = 'branding_cache';

/** Brand colours the UI themes against. Defaults mirror the neon palette. */
export interface BrandingState {
  displayName: string;
  appTagline: string | null;
  logoUrl: string | null;
  primary: string;
  secondary: string;
  accent: string;
  loaded: boolean;
}

const DEFAULT_STATE: BrandingState = {
  displayName: 'LogisticOS',
  appTagline: 'AI Agentic Last-Mile Delivery',
  logoUrl: null,
  primary: '#00E5FF',
  secondary: '#A855F7',
  accent: '#00FF88',
  loaded: false,
};

function fromDto(dto: BrandingDto): Omit<BrandingState, 'loaded'> {
  return {
    displayName: dto.display_name || DEFAULT_STATE.displayName,
    appTagline: dto.app_tagline ?? DEFAULT_STATE.appTagline,
    logoUrl: dto.logo_url ?? null,
    primary: dto.primary_color || DEFAULT_STATE.primary,
    secondary: dto.secondary_color || DEFAULT_STATE.secondary,
    accent: dto.accent_color || DEFAULT_STATE.accent,
  };
}

/** Emit the last-known brand from cache for instant cold-start theming. */
export const hydrateBrandingFromCache = createAsyncThunk(
  'branding/hydrate',
  async (): Promise<Partial<BrandingState> | null> => {
    try {
      const raw = await SecureStore.getItemAsync(CACHE_KEY);
      return raw ? (JSON.parse(raw) as Partial<BrandingState>) : null;
    } catch {
      return null;
    }
  },
);

/** Pull the tenant brand from the network and persist it. */
export const fetchBranding = createAsyncThunk(
  'branding/fetch',
  async (): Promise<Omit<BrandingState, 'loaded'> | null> => {
    const dto = await fetchBrandingApi();
    if (!dto) return null;
    const next = fromDto(dto);
    try {
      await SecureStore.setItemAsync(CACHE_KEY, JSON.stringify(next));
    } catch {
      /* cache write is best-effort */
    }
    return next;
  },
);

const brandingSlice = createSlice({
  name: 'branding',
  initialState: DEFAULT_STATE,
  reducers: {
    resetBranding: () => ({ ...DEFAULT_STATE, loaded: true }),
  },
  extraReducers: (builder) => {
    builder
      .addCase(hydrateBrandingFromCache.fulfilled, (state, action: PayloadAction<Partial<BrandingState> | null>) => {
        if (action.payload) Object.assign(state, action.payload);
      })
      .addCase(fetchBranding.fulfilled, (state, action) => {
        if (action.payload) Object.assign(state, action.payload);
        state.loaded = true;
      })
      .addCase(fetchBranding.rejected, (state) => {
        state.loaded = true;
      });
  },
});

export const { resetBranding } = brandingSlice.actions;
export default brandingSlice.reducer;

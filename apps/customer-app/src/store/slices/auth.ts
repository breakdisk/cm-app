import { createSlice, PayloadAction } from '@reduxjs/toolkit';

export type KycStatus = 'none' | 'pending' | 'verified' | 'rejected';
export type IdType = 'passport' | 'emirates_id' | 'drivers_license';

export interface AuthState {
  token: string | null;
  refreshToken: string | null;
  customerId: string | null;
  name: string | null;
  phone: string | null;
  email: string | null;
  kycStatus: KycStatus;
  onboardingStep: 'phone' | 'profile' | 'kyc' | 'complete';
  loyaltyPoints: number;
  loyaltyPts?: number;
  isGuest: boolean;
  verificationTier?: 'none' | 'verified';
}

const initialState: AuthState = {
  token: null,
  refreshToken: null,
  customerId: null,
  name: null,
  phone: null,
  email: null,
  kycStatus: 'none',
  onboardingStep: 'phone',
  loyaltyPoints: 0,
  loyaltyPts: 0,
  isGuest: true,
  verificationTier: 'none',
};

const authSlice = createSlice({
  name: 'auth',
  initialState,
  reducers: {
    setCredentials: (state, action: PayloadAction<Partial<AuthState>>) => {
      Object.assign(state, action.payload);
    },
    setPhone: (state, action: PayloadAction<string>) => {
      state.phone = action.payload;
      state.onboardingStep = 'profile';
    },
    setProfile: (state, action: PayloadAction<{ name: string; email: string; customerId?: string }>) => {
      state.name = action.payload.name;
      state.email = action.payload.email;
      if (action.payload.customerId) state.customerId = action.payload.customerId;
      state.onboardingStep = 'kyc';
    },
    submitKyc: (state, _action?: PayloadAction<{ idType: IdType }>) => {
      state.kycStatus = 'pending';
      state.onboardingStep = 'complete';
      state.isGuest = false;
      state.verificationTier = 'none';
    },
    /** User skipped KYC during onboarding — enters the app unverified. */
    skipKyc: (state) => {
      state.onboardingStep = 'complete';
      state.isGuest = false;
      // kycStatus stays 'none' — distinct from 'pending' (which means submitted)
    },
    approveKyc: (state) => {
      state.kycStatus = 'verified';
      state.verificationTier = 'verified';
    },
    rejectKyc: (state) => {
      state.kycStatus = 'rejected';
      state.verificationTier = 'none';
    },
    addLoyaltyPoints: (state, action: PayloadAction<number>) => {
      state.loyaltyPoints += action.payload;
      state.loyaltyPts = state.loyaltyPoints;
    },
    addLoyaltyPts: (state, action: PayloadAction<number>) => {
      state.loyaltyPoints += action.payload;
      state.loyaltyPts = state.loyaltyPoints;
    },
    logout: (state) => {
      return initialState;
    },
  },
});

export const {
  setCredentials,
  setPhone,
  setProfile,
  submitKyc,
  skipKyc,
  approveKyc,
  rejectKyc,
  addLoyaltyPoints,
  addLoyaltyPts,
  logout
} = authSlice.actions;
export default authSlice.reducer;

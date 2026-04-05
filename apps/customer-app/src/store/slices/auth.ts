import { createSlice, PayloadAction } from '@reduxjs/toolkit';

export interface AuthState {
  token: string | null;
  refreshToken: string | null;
  customerId: string | null;
  name: string | null;
  phone: string | null;
  email: string | null;
  kycStatus: 'pending' | 'submitted' | 'verified' | 'rejected';
  onboardingStep: 'phone' | 'profile' | 'kyc' | 'complete';
  loyaltyPoints: number;
}

const initialState: AuthState = {
  token: null,
  refreshToken: null,
  customerId: null,
  name: null,
  phone: null,
  email: null,
  kycStatus: 'pending',
  onboardingStep: 'phone',
  loyaltyPoints: 0,
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
    setProfile: (state, action: PayloadAction<{ name: string; email: string }>) => {
      state.name = action.payload.name;
      state.email = action.payload.email;
      state.onboardingStep = 'kyc';
    },
    submitKYC: (state) => {
      state.kycStatus = 'submitted';
    },
    addLoyaltyPoints: (state, action: PayloadAction<number>) => {
      state.loyaltyPoints += action.payload;
    },
    logout: (state) => {
      return initialState;
    },
  },
});

export const { setCredentials, setPhone, setProfile, submitKYC, addLoyaltyPoints, logout } = authSlice.actions;
export default authSlice.reducer;

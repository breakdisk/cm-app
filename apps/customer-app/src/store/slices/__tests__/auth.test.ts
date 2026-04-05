import authReducer, { setCredentials, logout } from '../auth';
import { AuthState } from '../auth';

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

describe('Auth Slice', () => {
  test('setCredentials updates token and profile', () => {
    const state = authReducer(initialState, setCredentials({
      token: 'jwt-token',
      customerId: 'cust-123',
      name: 'John Doe',
      phone: '+639123456789',
      email: 'john@example.com',
      kycStatus: 'verified',
    }));
    expect(state.token).toBe('jwt-token');
    expect(state.name).toBe('John Doe');
    expect(state.kycStatus).toBe('verified');
  });

  test('logout clears all auth state', () => {
    const preLogout = { ...initialState, token: 'some-token', customerId: 'cust-123' };
    const state = authReducer(preLogout, logout());
    expect(state.token).toBeNull();
    expect(state.customerId).toBeNull();
  });
});

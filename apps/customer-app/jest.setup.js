// Safe-area insets need a `<SafeAreaProvider>` above them, which a unit test
// rendering one screen in isolation does not have — `useSafeAreaInsets()` then
// throws "No safe area value available". The library ships a mock for exactly
// this; using it beats wrapping every screen test in a provider it does not
// otherwise care about.
jest.mock('react-native-safe-area-context', () =>
  require('react-native-safe-area-context/jest/mock').default,
);

// Axios is mocked globally, not per-suite.
//
// Its fetch adapter probes ReadableStream at *import* time
// (adapters/fetch.js -> getFetch -> test -> cancel), and Expo's stream
// polyfill throws "Cannot cancel a stream that already has a reader" when it
// does. Anything that transitively imports the api client therefore dies on
// load — and the Redux store does, via store/slices/branding -> api/branding
// -> api/client. That is a whole-app import graph, so patching it suite by
// suite is whack-a-mole; the module is stubbed once, here.
//
// A suite that needs to assert on axios itself (services/api/client) declares
// its own factory, which takes precedence over this one.
jest.mock('axios', () => {
  const instance = {
    defaults: { baseURL: '', headers: { common: {} } },
    interceptors: {
      request:  { use: jest.fn(), eject: jest.fn() },
      response: { use: jest.fn(), eject: jest.fn() },
    },
    get: jest.fn(() => Promise.resolve({ data: {} })),
    post: jest.fn(() => Promise.resolve({ data: {} })),
    put: jest.fn(() => Promise.resolve({ data: {} })),
    patch: jest.fn(() => Promise.resolve({ data: {} })),
    delete: jest.fn(() => Promise.resolve({ data: {} })),
    request: jest.fn(() => Promise.resolve({ data: {} })),
  };
  return {
    __esModule: true,
    default: { create: jest.fn(() => instance), ...instance },
    create: jest.fn(() => instance),
    isAxiosError: jest.fn(() => false),
  };
});

// Jest setup for React Native + Expo
jest.mock('expo-linear-gradient', () => {
  const React = require('react');
  return {
    LinearGradient: ({ children, ...props }) => React.createElement('div', props, children),
  };
});

jest.mock('@expo/vector-icons', () => ({
  MaterialIcons: () => null,
  Ionicons: () => null,
}));

// `virtual: true` because react-native-reanimated is NOT a dependency of this
// app — the mock exists defensively. Without it Jest throws "Cannot find module
// 'react-native-reanimated' from 'jest.setup.js'" before a single test runs,
// which is what every mobile test did here until the job stopped being skipped.
jest.mock('react-native-reanimated', () => {
  const mockAnimation = {
    delay: (ms) => mockAnimation,
    springify: () => mockAnimation,
  };
  return {
    Animated: {
      View: require('react-native').View,
    },
    FadeInDown: mockAnimation,
    FadeInUp: mockAnimation,
    FadeIn: mockAnimation,
  };
}, { virtual: true });

// Mock react-redux for testing
jest.mock('react-redux', () => {
  const View = require('react-native').View;
  return {
    Provider: View,
    useDispatch: () => jest.fn(),
    useSelector: jest.fn((selector) => {
      const mockState = {
        auth: {
          name: 'Test User',
          loyaltyPoints: 1000,
        },
        shipments: {
          list: [],
          byAwb: {},
          loading: false,
          error: null,
          pagination: { skip: 0, limit: 20, total: 0 },
        },
        tracking: {
          byAwb: {},
          loading: {},
          error: {},
          lastUpdated: {},
          history: [],
        },
        prefs: {},
        addresses: [],
      };
      return selector(mockState);
    }),
    connect: () => (Component) => Component,
  };
});

// Mock expo-secure-store
jest.mock('expo-secure-store', () => ({
  getItemAsync: jest.fn().mockResolvedValue(null),
  setItemAsync: jest.fn().mockResolvedValue(undefined),
  deleteItemAsync: jest.fn().mockResolvedValue(undefined),
}));

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
});

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

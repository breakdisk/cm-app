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

jest.mock('react-native-reanimated', () => ({
  Animated: {
    View: require('react-native').View,
  },
  FadeInDown: {
    springify: () => ({}),
  },
  FadeInUp: {
    delay: () => ({
      springify: () => ({}),
    }),
  },
}));

// Mock react-redux for testing
jest.mock('react-redux', () => {
  const View = require('react-native').View;
  return {
    Provider: View,
    useDispatch: () => jest.fn(),
    useSelector: () => ({
      auth: {
        name: 'Test User',
        loyaltyPoints: 1000,
      },
      shipments: {
        list: [],
      },
    }),
    connect: () => (Component) => Component,
  };
});

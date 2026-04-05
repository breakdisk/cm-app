// Jest setup for React Native + Expo
jest.mock('expo-linear-gradient', () => {
  const React = require('react');
  return {
    LinearGradient: ({ children, ...props }) => React.createElement('div', props, children),
  };
});

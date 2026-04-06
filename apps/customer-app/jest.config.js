module.exports = {
  preset: 'jest-expo',
  testEnvironment: 'node',
  testPathIgnorePatterns: ['/node_modules/', '/dist/', '/.expo/'],
  moduleFileExtensions: ['ts', 'tsx', 'js', 'jsx', 'json'],
  setupFilesAfterEnv: ['<rootDir>/jest.setup.js'],
  collectCoverageFrom: [
    'src/**/*.{ts,tsx}',
    '!src/**/*.d.ts',
    '!src/**/index.ts',
  ],
  testMatch: [
    '**/__tests__/**/*.test.{ts,tsx}',
  ],
  moduleNameMapper: {
    '^@/(.*)$': '<rootDir>/src/$1',
    '\\.(css|less|scss|sass)$': 'identity-obj-proxy',
  },
  transform: {
    '^.+\\.(ts|tsx)$': 'babel-jest',
  },
  transformIgnorePatterns: [
    'node_modules/(?!(react-redux|redux|@redux|expo-modules-core|react-native|@react-native|expo|expo-asset|expo-constants|expo-font|expo-linear-gradient|@expo|react-native-reanimated|expo-image-picker|expo-sqlite|expo-modules)/)',
  ],
};

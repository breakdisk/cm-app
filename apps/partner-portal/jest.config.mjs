// `npm test` was wired to `jest --passWithNoTests` with no config, so a
// TypeScript test could not run at all. next/jest supplies the SWC transform
// the app already builds with. Node environment: these are logic tests; a test
// that needs the DOM sets `@jest-environment jsdom` for itself.
import nextJest from "next/jest.js";

const createJestConfig = nextJest({ dir: "./" });

export default createJestConfig({
  testEnvironment: "node",
  moduleNameMapper: { "^@/(.*)$": "<rootDir>/src/$1" },
  // Relative globs on purpose: "<rootDir>/src/..." expands to a Windows path whose
  // backslashes the glob reads as escapes, and nothing matches.
  testMatch: ["**/src/**/*.test.ts", "**/src/**/*.test.tsx"],
});

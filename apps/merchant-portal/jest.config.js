const nextJest = require("next/jest");

// next/jest wires the SWC TypeScript transform and the "@/" path alias.
const createJestConfig = nextJest({ dir: "./" });

module.exports = createJestConfig({
  testEnvironment: "node",
  modulePathIgnorePatterns: ["<rootDir>/.next/"],
  moduleNameMapper: { "^@/(.*)$": "<rootDir>/src/$1" },
});

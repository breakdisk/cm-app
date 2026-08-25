/**
 * The admin portal's first working jest config.
 *
 * `jest` has been in devDependencies and `npm test -- --coverage --ci
 * --passWithNoTests` has been the `Unit Tests (admin-portal)` job in
 * `CI — Frontend` since this app was created, with no config and no test files
 * behind either. The job exited 0 every run without executing anything, and
 * adding a test file without this config does not fix that — jest falls back to
 * babel, which cannot parse TypeScript, and reports `Tests: 0 total`.
 *
 * `next/jest` is what makes it work: it wires Next's own SWC transform, so
 * TypeScript and the `@/*` alias are handled without adding babel, ts-jest or
 * any new package.
 *
 * `testEnvironment: "node"` on purpose. The suite covers pure functions with no
 * DOM, and jsdom is a separate dependency this app does not have.
 */
const nextJest = require("next/jest");

const createJestConfig = nextJest({ dir: "./" });

/**
 * No `testMatch` override on purpose. Jest's default already finds
 * `*.test.ts(x)`, and `next/jest` supplies `testPathIgnorePatterns` for
 * `node_modules` and `.next`.
 *
 * A `<rootDir>`-prefixed glob looks tidier and silently matches nothing on
 * Windows: `<rootDir>` expands to a mixed-separator path
 * (`D:/…\.claude/worktrees/…`) and micromatch reads the `\.` as an escaped
 * literal rather than a separator. The failure mode is `No tests found,
 * exiting with code 0` — indistinguishable from having written no tests, which
 * is the exact green no-op this config exists to end.
 */
module.exports = createJestConfig({
  testEnvironment: "node",
  moduleNameMapper: { "^@/(.*)$": "<rootDir>/src/$1" },
});

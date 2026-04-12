#!/usr/bin/env node
/**
 * One-time script to set a Firebase custom role claim on a user.
 *
 * Usage:
 *   node scripts/set-admin-claim.js <USER_UID> <ROLE>
 *
 * Roles: merchant | admin | partner | customer
 *
 * Requirements:
 *   - FIREBASE_SERVICE_ACCOUNT_JSON env var (base64-encoded service account JSON)
 *     OR place service-account.json in the project root (never commit it)
 *   - npm install firebase-admin (already installed in apps/landing)
 */

const admin = require("firebase-admin");

const uid  = process.argv[2];
const role = process.argv[3];

if (!uid || !role) {
  console.error("Usage: node scripts/set-admin-claim.js <USER_UID> <ROLE>");
  process.exit(1);
}

const VALID_ROLES = ["merchant", "admin", "partner", "customer"];
if (!VALID_ROLES.includes(role)) {
  console.error(`Invalid role '${role}'. Must be one of: ${VALID_ROLES.join(", ")}`);
  process.exit(1);
}

let serviceAccount;
if (process.env.FIREBASE_SERVICE_ACCOUNT_JSON) {
  serviceAccount = JSON.parse(
    Buffer.from(process.env.FIREBASE_SERVICE_ACCOUNT_JSON, "base64").toString("utf8")
  );
} else {
  try {
    serviceAccount = require("../service-account.json");
  } catch {
    console.error(
      "No service account found. Set FIREBASE_SERVICE_ACCOUNT_JSON env var or place service-account.json in project root."
    );
    process.exit(1);
  }
}

admin.initializeApp({ credential: admin.credential.cert(serviceAccount) });

admin.auth().setCustomUserClaims(uid, { role })
  .then(() => {
    console.log(`✓ Set role='${role}' on user ${uid}`);
    process.exit(0);
  })
  .catch((err) => {
    console.error("Failed:", err.message);
    process.exit(1);
  });

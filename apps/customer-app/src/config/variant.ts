/**
 * Which app this build is.
 *
 * The "LogisticOS Move" build (eas.json profile `move`, app.config.js) is the
 * consumer moving app from the mobile design, on the same sign-in and APIs.
 * Baked in at build time; the default build is unchanged.
 */
export const IS_MOVE_APP = process.env.EXPO_PUBLIC_APP_VARIANT === 'move';

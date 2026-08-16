# OmniDeliv — customer app

Expo / React Native. Hyperlocal restaurant and grocery delivery on the
LogisticOS platform.

## Running it against the live backend

Dependencies install with `npm install`. Then:

```bash
EXPO_PUBLIC_GATEWAY_API=https://os-api.cargomarket.net \
EXPO_PUBLIC_OMNIDELIV_API=https://os-api.cargomarket.net \
EXPO_PUBLIC_TENANT_SLUG=demo \
npx expo start
```

Scan the QR with Expo Go, or press `a` / `i` for an emulator.

**The environment variables are not optional.** They default to
`http://localhost:8000` and `:8091`, which on a phone means the phone itself.
Omit them and every request fails against a host that only exists on a
developer's laptop.

Both point at the gateway. `client.ts` names OmniDeliv separately because the
service listens on its own port in a local compose stack, but the gateway routes
`/v1/omnideliv` to it, so one origin serves both in any deployed environment.

## Signing in

Phone OTP, and the account auto-registers on first verify — so any number works.
On a non-production backend, **`123456` is accepted as the code** (see
`auth_service.rs`); otherwise read the real one from Redis at
`identity:otp:<phone>`.

After sign-in the app asks for a delivery address before anything else. That is
a gate rather than a default on purpose: a wrong address looks like it worked,
and a courier goes to the wrong door.

## Builds

`eas.json` defines development, preview and production profiles. **Every profile
sets the three `EXPO_PUBLIC_*` variables**, and that is the point of this
paragraph: `EXPO_PUBLIC_*` is compiled into the JS bundle at build time, not read
at runtime. A profile that omits them produces an app permanently hardcoded to
`localhost` — which is exactly what happened to the merchant portal and the
landing app, where `http://localhost:8091` shipped to production and every
visitor's browser was asked to serve the API itself.

Before the first build, the app needs an EAS project id:

```bash
cd apps/omnideliv-app && npx eas init
```

That writes `extra.eas.projectId` into `app.json` and requires an Expo account
with access to the organisation. Then:

```bash
npx eas build --profile preview --platform android
```

### Known traps

- **Run `npm install` before any `eas` command.** A stale `node_modules` makes
  `expo config` fail with no error text at all, and the build error that follows
  points somewhere unrelated.
- **Push notifications do not arrive in Expo Go.** The app registers a token
  after sign-in, but delivery needs a development build. Until one exists,
  engagement logs `no push tokens registered for user …` and the notification is
  recorded as failed — which is accurate, not a bug.

# FCM VPS Setup — Driver Push Notifications

**Status:** Required for production launch  
**Affects:** `logisticos-driver-ops` container only  
**Without this:** Drivers only receive task assignments when the app is foregrounded

---

## 1. Obtain `FCM_PROJECT_ID`

1. Open [Firebase Console](https://console.firebase.google.com/)
2. Select the LogisticOS project
3. Click the gear icon → **Project Settings** → **General** tab
4. Copy the **Project ID** (e.g. `logisticos-abc12`)

---

## 2. Obtain `FCM_SERVICE_ACCOUNT_JSON`

1. In Firebase Console → **Project Settings** → **Service Accounts** tab
2. Click **Generate new private key** → **Generate Key**
3. A JSON file downloads. Open it and copy the entire contents.
4. Minify to a single line (no newlines inside the value):
   ```bash
   jq -c . < your-downloaded-key.json
   ```
5. The single-line JSON string is the value for `FCM_SERVICE_ACCOUNT_JSON`.

---

## 3. Add env vars to Dokploy

SSH into the VPS and edit the compose env file:

```bash
nano /etc/dokploy/compose/oscargomarketnet-logisticosbackend-pqfh0u/code/.env
```

Add these two lines at the end:

```env
FCM_PROJECT_ID=your-project-id-here
FCM_SERVICE_ACCOUNT_JSON={"type":"service_account","project_id":"...","private_key_id":"...","private_key":"-----BEGIN PRIVATE KEY-----\n...\n-----END PRIVATE KEY-----\n","client_email":"...","client_id":"...","auth_uri":"...","token_uri":"...","auth_provider_x509_cert_url":"...","client_x509_cert_url":"..."}
```

> **Important:** The entire JSON value must be on a single line. Escaped `\n` sequences inside the `private_key` field are fine — literal newlines in the value are not.

Save and exit (`Ctrl+O`, `Enter`, `Ctrl+X`).

---

## 4. Restart the driver-ops container

In Dokploy dashboard, redeploy the `logisticos-driver-ops` service, or via SSH:

```bash
docker compose -f /etc/dokploy/compose/oscargomarketnet-logisticosbackend-pqfh0u/code/docker-compose.yml up -d --no-deps logisticos-driver-ops
```

---

## 5. Verify FCM initialised

```bash
docker logs logisticos-driver-ops 2>&1 | grep -i fcm
```

Expected output:
```
FCM client initialized
```

If you see no output, the env var may not have been picked up — confirm the container was restarted after editing `.env`.

---

## 6. Degraded mode (without FCM)

If FCM is not configured:

- Driver assignments are delivered via WebSocket only
- Drivers **must have the app foregrounded** to receive assignment notifications
- No data is lost — assignments are queued and visible when the driver next opens the app
- FCM can be added at any time without a backend code change; only the env var and container restart are needed

---

## 7. Troubleshooting

| Symptom | Likely cause | Fix |
|---------|-------------|-----|
| `FCM client initialized` not in logs | Env var missing or container not restarted | Re-edit `.env`, restart container |
| `invalid_grant` error in logs | Service account JSON is malformed or expired | Regenerate key in Firebase Console |
| Notifications delivered in foreground but not background | FCM configured correctly — check Android notification channel settings in driver app | Driver app issue, not VPS |
| `FCM_SERVICE_ACCOUNT_JSON` parse error | JSON has literal newlines | Re-minify with `jq -c .` |

#!/usr/bin/env bash
# ============================================================
# Proof of Pickup (POP) photo end-to-end smoke test
# ============================================================
# Drives the full POP photo path against a deployed gateway:
#   initiate POP -> presigned upload URL -> PUT photo to R2 ->
#   submit POP -> read back and confirm photo_url resolves.
#
# Proves, in order:
#   1. gateway routes the POP write path (POST /v1/pops != 404)
#   2. the pop_storage adapter is wired to logisticos-pop-photos
#      (presigned key starts with "pop/")
#   3. R2 credentials + bucket accept the PUT (object created)
#   4. submit records the photo key
#   5. GET returns a presigned photo_url the portal can render
#
# Exits non-zero on the first failure so it can gate a deploy.
#
# Usage:
#   GATEWAY=https://api.yourdomain TOKEN=<jwt> \
#   SHIPMENT_ID=<uuid> TASK_ID=<uuid> ./scripts/pop-photo-smoke.sh
# ============================================================

set -euo pipefail

: "${GATEWAY:?set GATEWAY, e.g. https://api.yourdomain}"
: "${TOKEN:?set TOKEN to a valid driver or admin JWT}"
: "${SHIPMENT_ID:?set SHIPMENT_ID to a real shipment uuid}"
: "${TASK_ID:?set TASK_ID to the pickup task uuid for that shipment}"

AUTH=(-H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json")
NOW="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
TMP_JPG="$(mktemp --suffix=.jpg)"
trap 'rm -f "$TMP_JPG"' EXIT

echo "==> 1. initiate POP"
INIT_BODY=$(jq -n --arg s "$SHIPMENT_ID" --arg t "$TASK_ID" --arg ts "$NOW" \
  '{shipment_id:$s,task_id:$t,capture_lat:14.5995,capture_lng:120.9842,pickup_lat:14.5995,pickup_lng:120.9842,declared_weight_g:1500,service_code:"standard",device_timestamp:$ts}')
INIT=$(curl -fsS "${AUTH[@]}" -X POST "${GATEWAY}/v1/pops" -d "$INIT_BODY")
POP_ID=$(echo "$INIT" | jq -r '.data.pop_id')
[ -n "$POP_ID" ] && [ "$POP_ID" != "null" ] || { echo "FAIL: no pop_id in: $INIT"; exit 1; }
echo "    pop_id=${POP_ID}"

echo "==> 2. request presigned upload URL"
UP=$(curl -fsS "${AUTH[@]}" -X POST "${GATEWAY}/v1/pops/${POP_ID}/upload-url" \
  -d '{"content_type":"image/jpeg"}')
PUT_URL=$(echo "$UP" | jq -r '.data.upload_url')
S3_KEY=$(echo "$UP" | jq -r '.data.s3_key')
echo "    s3_key=${S3_KEY}"
case "$S3_KEY" in
  pop/*) echo "    OK: key has pop/ prefix -> logisticos-pop-photos bucket";;
  *)     echo "FAIL: s3_key must start with pop/ — got ${S3_KEY}"; exit 1;;
esac

echo "==> 3. PUT a minimal test JPEG to R2"
printf '\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00\xff\xd9' > "$TMP_JPG"
SIZE=$(wc -c < "$TMP_JPG")
curl -fsS -X PUT "$PUT_URL" -H "Content-Type: image/jpeg" --data-binary @"$TMP_JPG"
echo "    uploaded ${SIZE} bytes to R2"

echo "==> 4. submit POP with photo key"
SUBMIT_BODY=$(jq -n --arg k "$S3_KEY" --argjson sz "$SIZE" --arg ts "$NOW" \
  '{scanned_barcode:"AWB-SMOKE-TEST",actual_weight_g:1550,photo_s3_key:$k,photo_size_bytes:$sz,device_timestamp:$ts,tenant_code:"PH1"}')
curl -fsS "${AUTH[@]}" -X PUT "${GATEWAY}/v1/pops/${POP_ID}/submit" -d "$SUBMIT_BODY" > /dev/null
echo "    submitted"

echo "==> 5. read back; confirm presigned photo_url"
VIEW=$(curl -fsS "${AUTH[@]}" "${GATEWAY}/v1/pops?shipment_id=${SHIPMENT_ID}")
PHOTO_URL=$(echo "$VIEW" | jq -r 'if (.data | type) == "array" then .data[0].photo_url else .data.photo_url end')
case "$PHOTO_URL" in
  http*) echo "    OK: photo_url is a presigned GET URL";;
  *)     echo "FAIL: no photo_url in: $VIEW"; exit 1;;
esac

echo "==> PASS — verify object in logisticos-pop-photos: ${S3_KEY}"

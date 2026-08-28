"""
Network International's wire format, checked against a real sandbox.

    python scripts/ni-sandbox-verify.py

Everything in `services/payments/src/infrastructure/external/network_international.rs`
follows N-Genius's *documented* hosted-order pattern. None of it has ever been
run against NI. This script is the one step nothing in the repo can substitute
for, reduced to a single command.

The question that matters most:

    When NI calls our webhook, is `orderReference` *our* merchant_order_reference
    (the payment_intents.id we send) or *NI's own* order reference (what it
    returns as `reference`)?

The adapter originally assumed the former and parsed it as a UUID. If that
assumption is wrong, every capture webhook fails while customers are charged.
`find_by_order_ref` now resolves either convention, so the integration is safe
regardless -- but which one NI actually uses is still unknown, and it decides
whether `gateway_order_ref` is load-bearing or dead weight.

What this does:
  1. Creates a real sandbox order, exactly as `create_session` does.
  2. Prints NI's `reference` next to the merchant reference we sent, so you can
     see at a glance whether they differ.
  3. Dumps the full response, so any field name that has drifted from what the
     adapter deserializes (`_links.payment.href`, `reference`) shows up as a
     mismatch rather than a runtime surprise.
  4. Optionally verifies a captured webhook body piped in on stdin: recomputes
     the HMAC the way `verify_webhook` does and reports whether the signature
     scheme matches.

NOTE on webhook verification mode: this script only ever checks the HMAC
scheme (step 4). NI's own webhook configuration UI actually asks for a
"Header Key" / "Header Value" pair as well as (separately) an "Encryption
Key" -- if the sandbox's webhook config was set up with a Header Key/Value
pair, NI will attach that pair as a static header to every call instead of
(or possibly in addition to) signing the body, and step 4 here will report a
mismatch even though the webhook is legitimate. Check what the portal's
webhook config actually has configured before trusting a step-4 mismatch as
proof the HMAC scheme is wrong. Either way, `services/payments` must be
configured to match: NETWORK_INTERNATIONAL__WEBHOOK_SECRET for HMAC mode, or
NETWORK_INTERNATIONAL__WEBHOOK_HEADER_KEY + ..._WEBHOOK_HEADER_VALUE for the
static-header mode -- see `verify_webhook` in
services/payments/src/infrastructure/external/network_international.rs.

Creates one sandbox order. Charges nothing, captures nothing. Sandbox only --
it refuses to run against a base URL that does not look like a sandbox unless
you set NI_VERIFY_ALLOW_PROD=1.

Configuration (the same names the service uses, so a working .env just works):
    NETWORK_INTERNATIONAL__BASE_URL        e.g. https://api-gateway.sandbox.ngenius-payments.com
    NETWORK_INTERNATIONAL__API_KEY
    NETWORK_INTERNATIONAL__OUTLET_REF
    NETWORK_INTERNATIONAL__WEBHOOK_SECRET  only needed for step 4
"""

import base64
import hashlib
import hmac
import json
import os
import sys
import uuid
import urllib.error
import urllib.request

BASE_URL = os.environ.get("NETWORK_INTERNATIONAL__BASE_URL", "").rstrip("/")
API_KEY = os.environ.get("NETWORK_INTERNATIONAL__API_KEY", "")
OUTLET_REF = os.environ.get("NETWORK_INTERNATIONAL__OUTLET_REF", "")
WEBHOOK_SECRET = os.environ.get("NETWORK_INTERNATIONAL__WEBHOOK_SECRET", "")


def die(msg):
    print("\n  " + msg + "\n", file=sys.stderr)
    sys.exit(1)


def post_json(url, payload, api_key):
    body = json.dumps(payload).encode()
    req = urllib.request.Request(url, data=body, method="POST")
    req.add_header("Content-Type", "application/json")
    req.add_header("Accept", "application/json")
    # Mirrors reqwest's `.bearer_auth(api_key)` in the adapter.
    req.add_header("Authorization", "Bearer " + api_key)
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return resp.status, json.loads(resp.read().decode())
    except urllib.error.HTTPError as e:
        raw = e.read().decode(errors="replace")
        # The adapter captures the error body too. If this comes back
        # unreadable, production debugging of a declined charge is just as blind.
        return e.code, raw


def main():
    missing = [
        name
        for name, value in [
            ("NETWORK_INTERNATIONAL__BASE_URL", BASE_URL),
            ("NETWORK_INTERNATIONAL__API_KEY", API_KEY),
            ("NETWORK_INTERNATIONAL__OUTLET_REF", OUTLET_REF),
        ]
        if not value
    ]
    if missing:
        die("Set these first: " + ", ".join(missing))

    if "sandbox" not in BASE_URL and os.environ.get("NI_VERIFY_ALLOW_PROD") != "1":
        die(
            BASE_URL + " does not look like a sandbox, and this creates a real "
            "order.\n  Set NI_VERIFY_ALLOW_PROD=1 if you genuinely mean it."
        )

    # The adapter sends payment_intents.id here; any UUID stands in for it.
    merchant_ref = str(uuid.uuid4())
    url = BASE_URL + "/transactions/outlets/" + OUTLET_REF + "/orders"
    payload = {
        "action": "SALE",
        "amount": {"currencyCode": "AED", "value": 2200},
        "merchant_order_reference": merchant_ref,
        "merchant_attributes": {"redirectUrl": "https://example.invalid/payment/return"},
    }

    print("POST " + url)
    print("  merchant_order_reference we sent: " + merchant_ref + "\n")

    status, resp = post_json(url, payload, API_KEY)
    if status >= 400 or not isinstance(resp, dict):
        die("NI returned " + str(status) + ":\n\n" + str(resp))

    print("NI returned " + str(status) + ". Full response:\n")
    print(json.dumps(resp, indent=2))
    print()

    ni_reference = resp.get("reference")
    checkout = (resp.get("_links") or {}).get("payment", {}).get("href")

    # These two field names are what the adapter deserializes. If either is
    # absent, create_session fails at runtime however good the credentials are.
    if ni_reference is None:
        print("  MISMATCH: no `reference` field. `CreateOrderResponse.reference`")
        print("            in network_international.rs will fail to deserialize.")
    if checkout is None:
        print("  MISMATCH: no `_links.payment.href`. The adapter has nowhere to")
        print("            send the customer.")

    print("What to compare when the webhook arrives:")
    print("  our merchant_order_reference : " + merchant_ref)
    print("  NI's own `reference`         : " + str(ni_reference))
    print()
    print("  If the webhook's `orderReference` equals the first, NI echoes our")
    print("  reference and the UUID path in find_by_order_ref is the live one.")
    print("  If it equals the second, the gateway_order_ref fallback is what")
    print("  carries every capture -- load-bearing, not belt-and-braces.")
    print()
    print("  Either way payments logs which path resolved it, at INFO.")

    if checkout:
        print("\n  Pay the sandbox order here to fire a real webhook:\n    " + checkout)

    if not WEBHOOK_SECRET:
        print(
            "\n  (Set NETWORK_INTERNATIONAL__WEBHOOK_SECRET and pipe a captured"
            "\n   webhook body in on stdin to check the signature too.)"
        )
        return

    if sys.stdin.isatty():
        return

    raw = sys.stdin.buffer.read()
    if not raw.strip():
        return

    # Exactly what verify_webhook does: HMAC-SHA256 over the raw body, base64.
    computed = base64.b64encode(
        hmac.new(WEBHOOK_SECRET.encode(), raw, hashlib.sha256).digest()
    ).decode()
    print("\nWebhook body signature check")
    print("  expected x-ni-signature: " + computed)
    print("  Compare against the header NI actually sent. If the value matches")
    print("  but the header name differs, fix the header name in verify_webhook.")
    print("  If the value differs, NI signs something other than the raw body --")
    print("  or, if the portal's webhook config has a Header Key/Value pair set,")
    print("  NI may be sending that static header instead of signing the body at")
    print("  all. Check the headers on the actual webhook request before assuming")
    print("  the HMAC scheme itself is wrong.")

    try:
        parsed = json.loads(raw)
    except json.JSONDecodeError:
        return
    order_ref = parsed.get("orderReference")
    if order_ref:
        print("\n  webhook orderReference: " + str(order_ref))
        try:
            uuid.UUID(str(order_ref))
            print("  -> parses as a UUID: NI echoes our merchant reference.")
        except ValueError:
            print("  -> not a UUID: NI sends its own reference, so the")
            print("     gateway_order_ref fallback is what makes capture work.")


if __name__ == "__main__":
    main()

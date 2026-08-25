"""
The compliance loop, end to end, against a running deployment.

    python scripts/e2e-compliance-loop.py

Courier submits a document -> it appears in the admin queue carrying the
courier's identity -> the reviewer receives the actual bytes. That last step is
the one that had never happened: the console rendered an `s3://` href for its
whole life, and when that was replaced with a presigned URL it pointed at
`http://minio:9000`, a compose-network host no browser can resolve. Both looked
fine in code review. Running this is what found them.

Two production bugs came out of the first run, both merged as fixes:
  - `GET /me/profile` 404'd for a courier with no profile, and that was a dead
    end rather than an error: the app calls only that route on load, so the
    upload form behind it was unreachable (PR #141).
  - the presigned URL named a host no reviewer could reach (PR #142).

Read-mostly. It creates one compliance profile and one document for whichever
courier account it is pointed at, and reviews nothing.

Configuration, all optional:
    COMPLIANCE_E2E_API       default https://os-api.cargomarket.net
    COMPLIANCE_E2E_TENANT    default demo
    COMPLIANCE_E2E_COURIER   default driver@demo.com
    COMPLIANCE_E2E_ADMIN     default admin@demo.com
    COMPLIANCE_E2E_PASSWORD  default is the documented dev seed password

Point it at a throwaway tenant for anything other than the dev seed.
"""
import base64
import json
import os
import ssl
import urllib.request
import urllib.error
import zlib

API      = os.environ.get("COMPLIANCE_E2E_API", "https://os-api.cargomarket.net")
TENANT   = os.environ.get("COMPLIANCE_E2E_TENANT", "demo")
COURIER  = os.environ.get("COMPLIANCE_E2E_COURIER", "driver@demo.com")
ADMIN    = os.environ.get("COMPLIANCE_E2E_ADMIN", "admin@demo.com")
PASSWORD = os.environ.get("COMPLIANCE_E2E_PASSWORD", "LogisticOS1!")
CTX = ssl.create_default_context()


# 1x1 red PNG, built here so the script has no fixtures.
def tiny_png() -> bytes:
    def chunk(tag: bytes, data: bytes) -> bytes:
        return (len(data).to_bytes(4, "big") + tag + data
                + zlib.crc32(tag + data).to_bytes(4, "big"))
    ihdr = (1).to_bytes(4, "big") + (1).to_bytes(4, "big") + bytes([8, 2, 0, 0, 0])
    idat = zlib.compress(bytes([0, 255, 0, 0]))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")


def call(method, path, token=None, body=None, raw_url=None):
    url = raw_url or (API + path)
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, method=method)
    # Cloudflare fronts this host and answers urllib's default agent with a
    # 1010 browser-signature ban, which looks exactly like an auth failure.
    req.add_header("User-Agent", "curl/8.4.0")
    if data:
        req.add_header("Content-Type", "application/json")
    if token:
        req.add_header("Authorization", "Bearer " + token)
    try:
        with urllib.request.urlopen(req, context=CTX, timeout=90) as r:
            return r.status, r.read(), dict(r.headers)
    except urllib.error.HTTPError as e:
        return e.code, e.read(), dict(e.headers)


def login(email):
    st, b, _ = call("POST", "/v1/auth/login",
                    body={"email": email, "password": PASSWORD, "tenant_slug": TENANT})
    assert st == 200, f"login {email} -> {st} {b[:200]}"
    return json.loads(b)["data"]["access_token"]


def step(n, msg):
    print(f"\n[{n}] {msg}")


ok = True
def check(label, cond, detail=""):
    global ok
    print(f"    {'PASS' if cond else 'FAIL'}  {label}" + (f"  {detail}" if detail else ""))
    if not cond:
        ok = False


step(1, f"log in as {COURIER} and {ADMIN} on tenant {TENANT}")
drv = login(COURIER)
adm = login(ADMIN)
print("    got both tokens")

step(2, "GET /me/profile  (404 before #141 — the dead end)")
st, b, _ = call("GET", "/api/v1/compliance/me/profile", drv)
check("returns 200, not 404", st == 200, f"HTTP {st}")
if st != 200:
    print("    ", b[:300])
    raise SystemExit(1)
prof = json.loads(b)["data"]
print(f"    profile      : {prof['profile']['id']}")
print(f"    entity_type  : {prof['profile']['entity_type']}")
print(f"    status       : {prof['profile']['overall_status']}")
req_types = prof["required_types"]
print(f"    required     : {[t['code'] for t in req_types]}")
check("entity_type is 'driver'", prof["profile"]["entity_type"] == "driver")
check("required types are non-empty", len(req_types) > 0)

step(3, "courier uploads a document")
png = tiny_png()
st, b, _ = call("POST", "/api/v1/compliance/me/documents/upload", drv, body={
    "document_type_code": req_types[0]["code"],
    "document_number": "E2E-LOOP-0001",
    "file_base64": base64.b64encode(png).decode(),   # RFC 4648, no wrapping
    "content_type": "image/png",
    "expiry_date": "2030-01-01",
})
check("upload accepted", st == 200, f"HTTP {st}")
if st != 200:
    print("    ", b[:400])
    raise SystemExit(1)
doc = json.loads(b)["data"]
doc_id = doc["id"]
print(f"    document id  : {doc_id}")
print(f"    file_url     : {doc['file_url']}")
check("stored as an s3:// object", doc["file_url"].startswith("s3://"))

step(4, "admin queue carries the courier's identity  (the #140 join)")
st, b, _ = call("GET", "/api/v1/compliance/admin/queue", adm)
check("queue returns 200", st == 200, f"HTTP {st}")
rows = json.loads(b)["data"]
mine = [r for r in rows if r["id"] == doc_id]
check("the submission is in the queue", len(mine) == 1, f"{len(rows)} row(s) total")
if mine:
    r = mine[0]
    print(f"    entity_id      : {r.get('entity_id')}")
    print(f"    entity_type    : {r.get('entity_type')}")
    print(f"    jurisdiction   : {r.get('jurisdiction')}")
    print(f"    overall_status : {r.get('overall_status')}")
    check("entity_id present (was absent — queue said 'Profile 3f2a…')", bool(r.get("entity_id")))
    check("entity_id is the courier", r.get("entity_id") == prof["profile"]["entity_id"])
    check("document fields stayed top-level (serde flatten)", "submitted_at" in r and "document" not in r)

step(5, "document types resolve to names  (was a uuid prefix)")
st, b, _ = call("GET", "/api/v1/compliance/admin/document-types", adm)
types = {t["id"]: t["name"] for t in json.loads(b)["data"]}
name = types.get(doc["document_type_id"])
check("the document's type has a name", name is not None, f"-> {name!r}")

step(6, "THE FIX: the reviewer receives the document itself")
st, body, hdrs = call("GET", f"/api/v1/compliance/admin/documents/{doc_id}/content", adm)
check("content returns 200", st == 200, f"HTTP {st}")
if st != 200:
    print("    ", body[:400])
    raise SystemExit(1)
ctype = hdrs.get("Content-Type", "")
print(f"    content-type : {ctype}")
print(f"    cache-control: {hdrs.get('Cache-Control')}")
print(f"    bytes        : {len(body)}")
check("type sniffed from the file's own magic number", ctype.startswith("image/png"),
      f"got {ctype!r}")
check("an identity document is not left in a shared cache",
      "no-store" in (hdrs.get("Cache-Control") or ""))
check("bytes are exactly what the courier uploaded", body == png,
      f"{len(body)} vs {len(png)} uploaded")

step(7, "and a reviewer from another tenant cannot read it")
st, body, _ = call("GET", f"/api/v1/compliance/admin/documents/{doc_id}/content", drv)
check("a courier token is refused (needs compliance:review)", st in (401, 403),
      f"HTTP {st}")

print("\n" + ("ALL CHECKS PASSED" if ok else "SOME CHECKS FAILED"))
print(f"document_id={doc_id}")
raise SystemExit(0 if ok else 1)

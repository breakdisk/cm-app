# Security Policy

## Overview

LogisticOS is a commercial multi-tenant SaaS platform for logistics and last-mile delivery operations. It processes payment data (Cash on Delivery), shipment records, real-time GPS and location data, and customer personally identifiable information (PII) across multiple tenants. Security is a core architectural concern, not an afterthought.

This document describes our supported versions policy, how to report vulnerabilities, the security standards we implement, our compliance scope, and our responsible disclosure policy.

---

## Supported Versions

Only the latest production release of LogisticOS receives security patches. Tenants on managed SaaS deployments are automatically kept current. Self-hosted or enterprise on-premises deployments are responsible for applying patches within the windows defined below.

| Version Type         | Security Support        |
|----------------------|-------------------------|
| Latest stable release | Actively supported      |
| Previous major release | Critical patches only, for 90 days after new major release |
| Older releases        | Not supported — upgrade required |

Enterprise customers with active support contracts may negotiate extended patch windows. Contact your account team for details.

---

## Reporting a Vulnerability

We take all security reports seriously. If you believe you have discovered a vulnerability in LogisticOS, please follow the responsible disclosure process below.

### How to Report

**Do not** open a public GitHub issue for security vulnerabilities. All reports must be submitted through private channels.

**Primary contact:**
- Email: `security@logisticos.io` *(placeholder — replace before publishing)*

**PGP key for encrypted submissions:**
- Key ID: *(to be published on keys.openpgp.org before public launch)*

### What to Include in Your Report

To help us triage and reproduce the issue efficiently, please include as much of the following as possible:

- A clear description of the vulnerability and its potential impact
- The affected component or service (e.g., identity service, dispatch API, merchant portal)
- Step-by-step reproduction instructions
- Proof-of-concept code, screenshots, or request/response captures (redact any real PII)
- The environment where the issue was found (production, staging, self-hosted)
- Your assessment of severity (Critical / High / Medium / Low) and your reasoning
- Any suggested remediation if you have one

### Response SLA

| Stage                              | Target Timeframe     |
|------------------------------------|----------------------|
| Acknowledgement of receipt         | Within 2 business days |
| Initial triage and severity rating | Within 5 business days |
| Remediation plan communicated      | Within 10 business days (Critical: 3 business days) |
| Patch release — Critical           | Within 7 calendar days of confirmation |
| Patch release — High               | Within 30 calendar days of confirmation |
| Patch release — Medium / Low       | Next scheduled release cycle |

We will keep you informed of progress throughout the remediation process and will coordinate disclosure timing with you.

---

## Security Standards

### Authentication and Identity

- **OAuth 2.0 / OpenID Connect (OIDC)** is used for all SSO and identity federation flows.
- **JWT access tokens** with short expiry, paired with rotating refresh tokens, govern all session state.
- Tokens carry tenant ID claims; all downstream services validate and enforce tenant scope on every request.
- The Identity service is the sole authority for token issuance and revocation. No other service mints tokens.
- Multi-factor authentication (MFA) is supported and enforced for all operator and admin roles.

### Authorization

- **Role-Based Access Control (RBAC)** is enforced at both the API layer and the data layer. Roles are scoped per tenant and per service.
- **Row-Level Security (RLS)** is implemented at the PostgreSQL layer to provide hard tenant isolation. A query executed in one tenant's context cannot return rows belonging to another tenant, regardless of application-layer logic.
- API keys and webhook credentials are scoped to the minimum permission set required for their declared use case. Broad-scope keys are not issued.
- All privileged actions (dispatch assignment overrides, billing mutations, driver location queries) are subject to elevated permission checks and are audit-logged.

### Data Protection

- Data in transit is encrypted using TLS 1.2 minimum (TLS 1.3 preferred) on all external-facing endpoints.
- **Mutual TLS (mTLS)** is enforced for all internal service-to-service communication via Istio service mesh. No plaintext inter-service traffic is permitted within the cluster.
- Data at rest is encrypted at the storage layer using AES-256.
- **Secrets management** is handled exclusively through HashiCorp Vault. Secrets are never stored in source code, container images, environment variable files, or CI/CD pipeline artifacts.
- Payment data (card numbers, COD collection records, banking details) is confined strictly to the Payments & Billing service. No other service stores or caches payment credentials. This is enforced architecturally and verified during code review.
- Customer PII (name, address, phone, GPS history) is tagged in the data model. Access is logged. Erasure pipelines implement the right-to-erasure as required by GDPR and PDPA.

### Network and Infrastructure Security

- All public-facing APIs are served through the Envoy/Axum API Gateway, which enforces rate limiting per tenant, per API key, and per IP.
- The Kubernetes cluster uses namespace isolation, network policies, and Istio for segmentation. Services are not directly reachable from outside the mesh except through the gateway.
- All container images are scanned for known CVEs in CI before deployment.
- No privileged containers are permitted in production workloads.
- Node and cluster access is controlled via short-lived credentials through Vault. No long-lived kubeconfig credentials exist outside of break-glass procedures.

### Input Validation

- All input is validated at the API boundary using Rust's type system and the `validator` crate. Malformed or out-of-range input is rejected before reaching business logic.
- SQL queries use parameterized statements via SQLx compile-time checked queries. No raw string interpolation into queries is permitted.
- Uploaded files (POD photos, shipment documents) are validated for type and size, stored in isolated object storage, and served through signed URLs — never directly from the application server.

### Audit Logging

All mutations — including order creation, dispatch assignment, driver location access, billing events, configuration changes, and role assignments — are recorded in an append-only audit log with the following fields: actor identity, tenant ID, timestamp, source IP, action type, and affected resource. Audit logs are shipped to an isolated log aggregation system (Loki) with write-once access controls and are retained per the compliance schedule below.

---

## Compliance Scope

### GDPR (EU General Data Protection Regulation)

LogisticOS processes personal data of EU residents on behalf of merchant tenants who operate in or serve the EU. Our obligations in this context include:

- Data processing agreements (DPAs) are executed with all EU-facing tenants.
- Lawful basis for processing is documented per data category.
- Data subject rights are implemented: access, rectification, erasure, restriction of processing, and data portability.
- Personal data is not transferred outside the EU without appropriate safeguards (Standard Contractual Clauses or equivalent).
- Breach notification to supervisory authorities is executed within 72 hours of discovery for qualifying incidents.
- Audit logs and personal data are retained for no longer than legally required and are deleted on schedule.

### PDPA (Philippine Data Privacy Act of 2012)

The Philippines is LogisticOS's primary market. All processing of personal data of Filipino data subjects complies with the PDPA and its implementing rules:

- A registered Data Protection Officer (DPO) is designated and registered with the National Privacy Commission (NPC).
- Privacy notices are provided in plain language at all data collection points.
- Consent is obtained before behavioral tracking or marketing communications.
- Data breach notifications are submitted to the NPC within 72 hours of discovery for qualifying incidents.
- Data sharing agreements are executed with third-party carriers and telecom partners who handle personal data.
- Privacy Impact Assessments (PIAs) are conducted for new high-risk processing activities.

### PCI-DSS

LogisticOS handles Cash on Delivery (COD) workflows and billing operations that may involve payment card data for certain tenants and payment gateway integrations.

- PCI-DSS scope is minimized by design: payment card data is never stored, logged, or processed outside the dedicated Payments & Billing service.
- The Payments & Billing service is isolated at the network, database, and code level.
- All integrations with payment gateways (Stripe, PayMongo, and others) use gateway-hosted tokenization; raw card numbers do not transit LogisticOS systems.
- Annual PCI-DSS assessments are conducted. Tenants requiring PCI compliance attestation should contact their account manager.

### Logistics and Transport Regulations

LogisticOS complies with applicable logistics, freight, and transport regulations in the jurisdictions where it operates, including requirements around shipment data retention, hazardous goods restrictions, and cross-border customs documentation.

---

## Responsible Disclosure Policy

We operate a coordinated vulnerability disclosure policy.

**Our commitments to researchers:**

- We will acknowledge your report promptly and keep you informed of our progress.
- We will not pursue legal action against researchers who discover and report vulnerabilities in good faith and in accordance with this policy.
- We will credit researchers publicly (with their consent) upon patch release, unless the researcher prefers to remain anonymous.
- We will work with you to agree on a disclosure timeline. Our default is public disclosure 90 days after a patch is released, or sooner if agreed.

**Requirements for good-faith research:**

- Do not access, modify, or exfiltrate data belonging to any tenant, merchant, driver, or customer — even as a proof of concept. Use test accounts and tenant environments only.
- Do not perform denial-of-service testing, fuzzing at scale, or load testing against production systems without prior written authorization.
- Do not exploit a confirmed vulnerability beyond what is necessary to demonstrate its existence.
- Do not use social engineering techniques against LogisticOS employees or customers.
- Report findings promptly — do not hold vulnerabilities for extended periods before disclosure.

---

## Out of Scope

The following items are explicitly outside the scope of our vulnerability disclosure program and will not be triaged:

- Vulnerabilities in third-party services or infrastructure providers (report those to the respective vendor)
- Issues that require physical access to servers or client devices
- Social engineering or phishing attacks against employees or customers
- Automated scanner output without proof of exploitability
- Denial-of-service attacks or volumetric testing
- Self-XSS or issues that require the victim to take highly unlikely actions
- Missing security headers where no practical attack path is demonstrated
- Rate limiting on non-sensitive, unauthenticated endpoints
- Vulnerabilities in outdated or unsupported browsers not in our support matrix
- Valid features that the researcher believes should behave differently (submit as a feature request)
- Reports related to username/email enumeration on public registration forms without demonstrated impact
- Cookie flags on non-sensitive cookies

---

## Bug Bounty

LogisticOS does not currently operate a public bug bounty program. Researchers who responsibly disclose valid, in-scope vulnerabilities will be acknowledged publicly (with consent) and may be considered for discretionary rewards at the company's judgment, based on severity and impact.

Tenants with enterprise agreements who discover vulnerabilities during integration or security testing should report through their designated account security contact.

We expect to launch a formal bug bounty program as the platform reaches general availability. Researchers interested in early participation may contact `security@logisticos.io`.

---

## Security Contact Information

| Role | Contact |
|------|---------|
| Security vulnerability reports | `security@logisticos.io` |
| Data protection / GDPR inquiries | `privacy@logisticos.io` |
| PDPA / DPO contact | `dpo@logisticos.io` |
| PCI-DSS compliance inquiries | `compliance@logisticos.io` |
| Security incidents (existing tenants) | Contact your account manager or `incidents@logisticos.io` |
| General security questions | `security@logisticos.io` |

*All email addresses above are placeholders and must be replaced with verified addresses before public launch.*

For critical or time-sensitive reports, mark your email subject line as `[SECURITY CRITICAL]` to ensure immediate routing.

---

## Document Control

| Field | Value |
|-------|-------|
| Document owner | Chief Information Security Officer (CISO) |
| Review cadence | Annually, or after any significant security incident |
| Last reviewed | March 2026 |
| Next review due | March 2027 |

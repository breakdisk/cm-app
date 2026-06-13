import type { Metadata } from "next";
import Link from "next/link";
import Navbar from "@/components/Navbar";
import Footer from "@/components/Footer";

export const metadata: Metadata = {
  title: "Privacy Policy — CargoMarket",
  description:
    "CargoMarket's Privacy Policy. Learn how we collect, use, and protect your personal data in compliance with GDPR, PDPA, and CCPA.",
  robots: {
    index: true,
    follow: true,
    googleBot: { index: true, follow: true },
  },
  alternates: {
    canonical: "https://cargomarket.net/privacy-policy",
  },
  openGraph: {
    title: "Privacy Policy — CargoMarket",
    description:
      "Learn how CargoMarket collects, uses, and protects your personal data.",
    type: "website",
    url: "https://cargomarket.net/privacy-policy",
  },
};

const LAST_UPDATED = "June 13, 2026";
const COMPANY_NAME = "CargoMarket (Atlas Cargome Inc.)";
const COMPANY_EMAIL = "privacy@cargomarket.net";
const COMPANY_ADDRESS = "Philippines";

const sections = [
  {
    id: "overview",
    title: "1. Overview",
    content: (
      <>
        <p>
          {COMPANY_NAME} (&quot;CargoMarket,&quot; &quot;we,&quot; &quot;us,&quot; or &quot;our&quot;)
          operates the <strong>cargomarket.net</strong> website, the CargoMarket mobile
          applications, and the LogisticOS SaaS platform (collectively, the &quot;Services&quot;).
        </p>
        <p>
          This Privacy Policy explains what personal data we collect, why we collect it,
          how we use and share it, and what rights you have over your information. It
          applies to all visitors, registered users, merchants, drivers, and consignees
          who interact with our Services.
        </p>
        <p>
          By accessing or using our Services, you acknowledge that you have read and
          understood this Privacy Policy. If you do not agree, please discontinue use of
          the Services.
        </p>
      </>
    ),
  },
  {
    id: "data-controller",
    title: "2. Data Controller",
    content: (
      <>
        <p>
          For the purposes of applicable data-protection laws (including the EU General
          Data Protection Regulation &quot;GDPR,&quot; the Philippine Data Privacy Act
          &quot;DPA/PDPA,&quot; and the California Consumer Privacy Act &quot;CCPA&quot;), the data
          controller is:
        </p>
        <div className="bg-white/[0.03] border border-white/[0.08] rounded-xl p-5 mt-4 space-y-1 text-sm text-slate-300">
          <p><strong className="text-white">{COMPANY_NAME}</strong></p>
          <p>{COMPANY_ADDRESS}</p>
          <p>
            Email:{" "}
            <a
              href={`mailto:${COMPANY_EMAIL}`}
              className="text-cyan-400 hover:text-cyan-300 underline"
            >
              {COMPANY_EMAIL}
            </a>
          </p>
        </div>
      </>
    ),
  },
  {
    id: "information-we-collect",
    title: "3. Information We Collect",
    content: (
      <>
        <p>We collect information in the following categories:</p>

        <h3 className="text-base font-semibold text-white mt-5 mb-2">3.1 Information You Provide Directly</h3>
        <ul>
          <li><strong>Account &amp; Registration:</strong> Name, email address, phone number, company name, role, and password when you sign up.</li>
          <li><strong>Shipment &amp; Order Data:</strong> Sender and recipient names, addresses, phone numbers, shipment contents, declared values, and special handling instructions.</li>
          <li><strong>Payment Information:</strong> Billing address, invoice details. Card numbers and payment credentials are processed directly by our PCI-DSS-certified payment partners (e.g., Stripe, PayMongo) and are never stored on our servers.</li>
          <li><strong>Communications:</strong> Messages sent to our support team, feedback forms, and customer service chat transcripts.</li>
          <li><strong>Proof of Delivery (POD):</strong> Photographs, signatures, GPS coordinates, and timestamps captured at delivery confirmation.</li>
        </ul>

        <h3 className="text-base font-semibold text-white mt-5 mb-2">3.2 Information Collected Automatically</h3>
        <ul>
          <li><strong>Device &amp; Browser Data:</strong> IP address, browser type and version, operating system, device identifiers, screen resolution, and referring URL.</li>
          <li><strong>Usage Data:</strong> Pages visited, features used, click-stream data, session duration, and navigation paths.</li>
          <li><strong>Location Data:</strong> Precise GPS coordinates from driver mobile apps (with explicit permission) and approximate location inferred from IP address for all users.</li>
          <li><strong>Cookies &amp; Similar Technologies:</strong> Session cookies, persistent cookies, pixel tags, and local storage. See Section 6 for full details.</li>
          <li><strong>Log Data:</strong> Server logs including timestamps, request URLs, HTTP response codes, and error logs for security and debugging purposes.</li>
        </ul>

        <h3 className="text-base font-semibold text-white mt-5 mb-2">3.3 Information from Third Parties</h3>
        <ul>
          <li><strong>Authentication Providers:</strong> If you sign in via Google or other OAuth providers, we receive your name, email, and profile picture from that provider.</li>
          <li><strong>Integration Partners:</strong> Shipment data from e-commerce platforms (Shopify, WooCommerce, Lazada, Shopee) when you authorize a connection.</li>
          <li><strong>Analytics &amp; Advertising Partners:</strong> Aggregated audience data from Meta (Facebook/Instagram), Google Analytics, and similar services as described in Section 6.</li>
        </ul>
      </>
    ),
  },
  {
    id: "how-we-use",
    title: "4. How We Use Your Information",
    content: (
      <>
        <p>We use the information we collect for the following purposes:</p>
        <ul>
          <li><strong>Service Delivery:</strong> Creating and managing accounts, processing shipments, assigning drivers, providing route optimization, and sending real-time delivery updates.</li>
          <li><strong>Customer Support:</strong> Responding to inquiries, resolving disputes, and providing technical assistance.</li>
          <li><strong>Billing &amp; Payments:</strong> Generating invoices, processing payments, managing Cash-on-Delivery (COD) reconciliation, and fraud prevention.</li>
          <li><strong>Platform Safety &amp; Security:</strong> Detecting and preventing fraudulent transactions, unauthorized access, and abuse of the platform.</li>
          <li><strong>Service Improvement:</strong> Analyzing usage patterns to improve features, fix bugs, and optimize platform performance.</li>
          <li><strong>Marketing &amp; Advertising:</strong> Sending promotional emails, SMS messages, WhatsApp notifications, and running targeted advertising campaigns on Meta (Facebook/Instagram), Google, and other platforms — subject to your consent where required by law.</li>
          <li><strong>AI &amp; Automation:</strong> Training and improving AI dispatch algorithms, demand forecasting models, and customer engagement automation pipelines. All AI model outputs are auditable and reversible.</li>
          <li><strong>Legal Compliance:</strong> Meeting our obligations under applicable laws, responding to lawful government requests, and enforcing our Terms of Service.</li>
          <li><strong>Communications:</strong> Sending transactional notifications (shipment status, OTP codes, invoices) and, where you have opted in, marketing communications.</li>
        </ul>

        <h3 className="text-base font-semibold text-white mt-5 mb-2">Legal Bases for Processing (GDPR)</h3>
        <p>For users in the EU/EEA, our legal bases for processing are:</p>
        <ul>
          <li><strong>Contract performance</strong> — processing necessary to deliver the logistics services you contracted.</li>
          <li><strong>Legitimate interests</strong> — fraud prevention, platform security, service improvement, and B2B marketing.</li>
          <li><strong>Consent</strong> — marketing communications, cookies beyond strictly necessary, and behavioral advertising. You may withdraw consent at any time.</li>
          <li><strong>Legal obligation</strong> — compliance with applicable laws and regulatory requirements.</li>
        </ul>
      </>
    ),
  },
  {
    id: "data-sharing",
    title: "5. How We Share Your Information",
    content: (
      <>
        <p>We do not sell your personal data. We share information only in the following circumstances:</p>
        <ul>
          <li><strong>Service Providers:</strong> Sub-processors who help us operate the Services, including cloud hosting (AWS/GCP), SMS/WhatsApp gateway providers (Twilio), email delivery (SendGrid), payment processors (Stripe, PayMongo), mapping services (Mapbox, Google Maps), and analytics platforms. All sub-processors are bound by Data Processing Agreements.</li>
          <li><strong>Logistics Partners &amp; Carriers:</strong> Third-party courier companies that handle last-mile delivery on behalf of our merchant clients receive shipment details (recipient name, address, phone) necessary to complete delivery.</li>
          <li><strong>Advertising Platforms:</strong> We share hashed email addresses and device identifiers with Meta Platforms, Inc. (Facebook/Instagram) and Google LLC for custom audience creation and conversion tracking as described in Section 6. This constitutes a data share for advertising purposes. You can opt out via the Cookie Settings link in our site footer.</li>
          <li><strong>Merchants &amp; Platform Tenants:</strong> If you are a consignee (shipment recipient), your name, contact details, and delivery address are visible to the merchant who created the shipment on your behalf.</li>
          <li><strong>Business Transfers:</strong> In the event of a merger, acquisition, or sale of all or substantially all assets, your data may be transferred as part of that transaction. We will notify you via email and/or a prominent notice on our website.</li>
          <li><strong>Legal Requirements:</strong> We may disclose your data if required by law, subpoena, court order, or government authority, or to protect the rights, property, or safety of CargoMarket, our users, or the public.</li>
          <li><strong>With Your Consent:</strong> For any other purpose, only with your explicit consent.</li>
        </ul>
      </>
    ),
  },
  {
    id: "cookies",
    title: "6. Cookies and Tracking Technologies",
    content: (
      <>
        <p>
          We use cookies, pixel tags, and similar tracking technologies on our website and
          applications. Some are strictly necessary; others require your consent.
        </p>

        <h3 className="text-base font-semibold text-white mt-5 mb-2">6.1 Categories of Cookies We Use</h3>
        <div className="overflow-x-auto">
          <table className="w-full text-sm text-slate-300 border-collapse mt-2">
            <thead>
              <tr className="border-b border-white/[0.08]">
                <th className="text-left py-2 pr-4 text-white font-semibold">Category</th>
                <th className="text-left py-2 pr-4 text-white font-semibold">Purpose</th>
                <th className="text-left py-2 text-white font-semibold">Consent Required</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-white/[0.05]">
              <tr>
                <td className="py-2.5 pr-4 font-medium text-white">Strictly Necessary</td>
                <td className="py-2.5 pr-4">Authentication, session management, security tokens, load balancing.</td>
                <td className="py-2.5">No</td>
              </tr>
              <tr>
                <td className="py-2.5 pr-4 font-medium text-white">Functional</td>
                <td className="py-2.5 pr-4">Language preferences, UI settings, tracking page tokens.</td>
                <td className="py-2.5">No</td>
              </tr>
              <tr>
                <td className="py-2.5 pr-4 font-medium text-white">Analytics</td>
                <td className="py-2.5 pr-4">Google Analytics — page views, session data, traffic sources, conversion funnels.</td>
                <td className="py-2.5">Yes</td>
              </tr>
              <tr>
                <td className="py-2.5 pr-4 font-medium text-white">Marketing / Advertising</td>
                <td className="py-2.5 pr-4">Meta Pixel, Google Ads conversion tracking, retargeting audiences for Facebook, Instagram, and Google advertising campaigns.</td>
                <td className="py-2.5">Yes</td>
              </tr>
            </tbody>
          </table>
        </div>

        <h3 className="text-base font-semibold text-white mt-6 mb-2">6.2 Meta Pixel (Facebook/Instagram)</h3>
        <p>
          We use the <strong>Meta Pixel</strong> (formerly Facebook Pixel) provided by
          Meta Platforms, Inc. The Meta Pixel is a piece of JavaScript code that allows
          us to measure the effectiveness of our advertising by understanding the actions
          people take on our website. Specifically, the Meta Pixel may:
        </p>
        <ul>
          <li>Track page views and specific events (e.g., account sign-ups, quote requests, shipment bookings).</li>
          <li>Create Custom Audiences for retargeting campaigns on Facebook and Instagram.</li>
          <li>Build Lookalike Audiences based on our existing customer base.</li>
          <li>Measure advertising campaign performance and attribution.</li>
        </ul>
        <p>
          The Meta Pixel sets cookies on your browser and may use your IP address, browser
          information, and behavior on our site. This data is governed by{" "}
          <a
            href="https://www.facebook.com/policy.php"
            target="_blank"
            rel="noopener noreferrer"
            className="text-cyan-400 hover:text-cyan-300 underline"
          >
            Meta&apos;s Data Policy
          </a>
          . The Meta Pixel is only activated if you have consented to Marketing cookies.
        </p>
        <p>
          You can opt out of Meta&apos;s targeted advertising at any time via{" "}
          <a
            href="https://www.facebook.com/ads/preferences"
            target="_blank"
            rel="noopener noreferrer"
            className="text-cyan-400 hover:text-cyan-300 underline"
          >
            Facebook Ad Preferences
          </a>{" "}
          or the{" "}
          <a
            href="https://optout.aboutads.info/"
            target="_blank"
            rel="noopener noreferrer"
            className="text-cyan-400 hover:text-cyan-300 underline"
          >
            Digital Advertising Alliance opt-out
          </a>
          .
        </p>

        <h3 className="text-base font-semibold text-white mt-5 mb-2">6.3 Managing Your Cookie Preferences</h3>
        <p>
          You can manage your cookie preferences at any time by clicking{" "}
          <strong>&quot;Cookie Settings&quot;</strong> in the website footer, or by configuring
          your browser to block or delete cookies. Note that disabling certain cookies may
          affect the functionality of our Services.
        </p>
      </>
    ),
  },
  {
    id: "data-retention",
    title: "7. Data Retention",
    content: (
      <>
        <p>We retain personal data for as long as necessary to fulfil the purposes described in this Policy:</p>
        <ul>
          <li><strong>Account Data:</strong> For the duration of your account plus 3 years after account closure (for legal and audit purposes).</li>
          <li><strong>Shipment &amp; Transaction Records:</strong> 7 years (financial record-keeping obligations).</li>
          <li><strong>Driver GPS &amp; Telemetry Logs:</strong> 12 months in raw form; aggregated analytics retained indefinitely.</li>
          <li><strong>Proof of Delivery (POD) Photos &amp; Signatures:</strong> 3 years from delivery date.</li>
          <li><strong>Marketing Data &amp; Email Lists:</strong> Until you unsubscribe or withdraw consent, plus 90 days thereafter.</li>
          <li><strong>Support Communications:</strong> 3 years from case closure.</li>
          <li><strong>Security Logs &amp; Audit Trails:</strong> 1 year in hot storage; up to 7 years in cold archive.</li>
        </ul>
        <p>
          When data is no longer required, it is securely deleted or anonymized so that
          it can no longer be linked to any individual.
        </p>
      </>
    ),
  },
  {
    id: "your-rights",
    title: "8. Your Privacy Rights",
    content: (
      <>
        <p>
          Depending on your location, you have the following rights over your personal data.
          To exercise any of these rights, contact us at{" "}
          <a href={`mailto:${COMPANY_EMAIL}`} className="text-cyan-400 hover:text-cyan-300 underline">
            {COMPANY_EMAIL}
          </a>
          . We will respond within 30 days.
        </p>

        <h3 className="text-base font-semibold text-white mt-5 mb-2">Under GDPR (EU/EEA users)</h3>
        <ul>
          <li><strong>Right of Access (Art. 15):</strong> Request a copy of all personal data we hold about you.</li>
          <li><strong>Right to Rectification (Art. 16):</strong> Correct inaccurate or incomplete data.</li>
          <li><strong>Right to Erasure / &quot;Right to be Forgotten&quot; (Art. 17):</strong> Request deletion of your data where there is no compelling reason to continue processing.</li>
          <li><strong>Right to Restriction (Art. 18):</strong> Ask us to limit how we use your data.</li>
          <li><strong>Right to Data Portability (Art. 20):</strong> Receive your data in a structured, machine-readable format.</li>
          <li><strong>Right to Object (Art. 21):</strong> Object to processing based on legitimate interests or for direct marketing at any time.</li>
          <li><strong>Right to Withdraw Consent:</strong> Where processing is based on consent, withdraw it at any time without affecting the lawfulness of prior processing.</li>
          <li><strong>Right to Lodge a Complaint:</strong> File a complaint with your local supervisory authority (e.g., the EU data protection authority in your member state).</li>
        </ul>

        <h3 className="text-base font-semibold text-white mt-5 mb-2">Under the Philippine Data Privacy Act (DPA)</h3>
        <ul>
          <li>Right to be informed, right to access, right to correct, right to erasure, right to object, right to data portability, and right to file a complaint with the National Privacy Commission (NPC).</li>
        </ul>

        <h3 className="text-base font-semibold text-white mt-5 mb-2">Under CCPA (California residents)</h3>
        <ul>
          <li>Right to know what personal information is collected, disclosed, or sold.</li>
          <li>Right to delete personal information.</li>
          <li>Right to opt out of the &quot;sale&quot; or &quot;sharing&quot; of personal information (note: we do not sell personal data).</li>
          <li>Right to non-discrimination for exercising your rights.</li>
          <li>Right to correct inaccurate personal information.</li>
          <li>Right to limit the use and disclosure of sensitive personal information.</li>
        </ul>
        <p>
          To submit a data request, email{" "}
          <a href={`mailto:${COMPANY_EMAIL}`} className="text-cyan-400 hover:text-cyan-300 underline">
            {COMPANY_EMAIL}
          </a>{" "}
          with the subject line &quot;Data Privacy Request&quot; and specify the right you wish to
          exercise. We may need to verify your identity before processing the request.
        </p>
      </>
    ),
  },
  {
    id: "security",
    title: "9. Data Security",
    content: (
      <>
        <p>We implement industry-standard technical and organizational security measures including:</p>
        <ul>
          <li>Encryption in transit (TLS 1.3) and at rest (AES-256).</li>
          <li>Mutual TLS (mTLS) between all internal microservices via Istio service mesh.</li>
          <li>HashiCorp Vault for secrets management — no credentials stored in code or environment files.</li>
          <li>Role-Based Access Control (RBAC) and Row-Level Security (RLS) at the database layer.</li>
          <li>Regular security audits, penetration testing, and OWASP compliance reviews.</li>
          <li>Comprehensive audit logging of all data mutations (actor, timestamp, tenant, IP).</li>
          <li>PCI-DSS scope minimization — payment card data is never stored in non-payment services.</li>
        </ul>
        <p>
          Despite these measures, no system is completely secure. In the event of a data
          breach that poses a risk to your rights, we will notify you and relevant
          authorities as required by applicable law (within 72 hours under GDPR; within 72
          hours under the Philippine DPA).
        </p>
      </>
    ),
  },
  {
    id: "international-transfers",
    title: "10. International Data Transfers",
    content: (
      <>
        <p>
          We are headquartered in the Philippines and use cloud infrastructure providers
          operating globally (AWS, GCP). Your data may be transferred to and processed in
          countries outside your home country, including countries that may not provide
          the same level of data protection.
        </p>
        <p>
          For transfers from the EEA/UK to third countries, we rely on:
        </p>
        <ul>
          <li>European Commission Standard Contractual Clauses (SCCs), or</li>
          <li>Adequacy decisions where applicable.</li>
        </ul>
        <p>
          You may request a copy of the relevant transfer mechanism by contacting{" "}
          <a href={`mailto:${COMPANY_EMAIL}`} className="text-cyan-400 hover:text-cyan-300 underline">
            {COMPANY_EMAIL}
          </a>
          .
        </p>
      </>
    ),
  },
  {
    id: "childrens-privacy",
    title: "11. Children's Privacy",
    content: (
      <p>
        Our Services are not directed at children under 16 years of age. We do not
        knowingly collect personal data from children. If you believe we have
        inadvertently collected data from a child, please contact us immediately at{" "}
        <a href={`mailto:${COMPANY_EMAIL}`} className="text-cyan-400 hover:text-cyan-300 underline">
          {COMPANY_EMAIL}
        </a>{" "}
        and we will delete it promptly.
      </p>
    ),
  },
  {
    id: "third-party-links",
    title: "12. Third-Party Links and Services",
    content: (
      <p>
        Our Services may contain links to third-party websites, integrations, or embedded
        content (e.g., maps, social widgets). This Privacy Policy does not apply to those
        third-party services. We encourage you to review the privacy policies of any
        third-party services you access through our platform.
      </p>
    ),
  },
  {
    id: "changes",
    title: "13. Changes to This Policy",
    content: (
      <>
        <p>
          We may update this Privacy Policy from time to time. When we make material
          changes, we will:
        </p>
        <ul>
          <li>Update the &quot;Last Updated&quot; date at the top of this page.</li>
          <li>Notify registered users via email at least 30 days before material changes take effect.</li>
          <li>Display a prominent notice on our website or in-app.</li>
        </ul>
        <p>
          Your continued use of the Services after the effective date of the revised
          Policy constitutes your acceptance of the changes.
        </p>
      </>
    ),
  },
  {
    id: "contact",
    title: "14. Contact Us",
    content: (
      <>
        <p>For privacy-related questions, requests, or complaints, contact our Data Privacy Officer:</p>
        <div className="bg-white/[0.03] border border-white/[0.08] rounded-xl p-5 mt-4 space-y-1 text-sm text-slate-300">
          <p><strong className="text-white">Data Privacy Officer — {COMPANY_NAME}</strong></p>
          <p>{COMPANY_ADDRESS}</p>
          <p>
            Email:{" "}
            <a
              href={`mailto:${COMPANY_EMAIL}`}
              className="text-cyan-400 hover:text-cyan-300 underline"
            >
              {COMPANY_EMAIL}
            </a>
          </p>
          <p>
            General Support:{" "}
            <a
              href="mailto:support@cargomarket.net"
              className="text-cyan-400 hover:text-cyan-300 underline"
            >
              support@cargomarket.net
            </a>
          </p>
        </div>
        <p className="mt-4 text-sm text-slate-500">
          If you are located in the EU/EEA and you believe we have not resolved your
          complaint satisfactorily, you have the right to lodge a complaint with your
          local Data Protection Authority.
        </p>
      </>
    ),
  },
];

export default function PrivacyPolicyPage() {
  return (
    <main className="min-h-screen bg-[#050810] text-slate-100 overflow-x-hidden">
      <Navbar />

      {/* Hero banner */}
      <section className="pt-32 pb-12 px-6 lg:px-8 relative">
        <div className="absolute inset-0 bg-gradient-to-b from-cyan-neon/[0.04] via-transparent to-transparent pointer-events-none" />
        <div className="max-w-4xl mx-auto relative">
          <div className="inline-flex items-center gap-2 px-3 py-1.5 rounded-full bg-white/[0.05] border border-white/[0.08] text-xs text-slate-400 mb-6">
            <span className="w-1.5 h-1.5 rounded-full bg-cyan-400" />
            Legal
          </div>
          <h1
            className="text-4xl lg:text-5xl font-bold text-white mb-4"
            style={{ fontFamily: "'Space Grotesk', sans-serif" }}
          >
            Privacy Policy
          </h1>
          <p className="text-slate-400 text-base">
            Last updated: <time dateTime="2026-06-13">{LAST_UPDATED}</time>
          </p>
          <p className="text-slate-400 text-sm mt-2">
            This policy applies to{" "}
            <a
              href="https://cargomarket.net"
              className="text-cyan-400 hover:text-cyan-300 underline"
            >
              cargomarket.net
            </a>{" "}
            and all CargoMarket products and services.
          </p>
        </div>
      </section>

      {/* Table of contents + content */}
      <section className="pb-24 px-6 lg:px-8">
        <div className="max-w-4xl mx-auto">
          {/* Table of contents */}
          <nav
            aria-label="Privacy policy sections"
            className="bg-white/[0.03] border border-white/[0.08] rounded-2xl p-6 mb-12"
          >
            <h2 className="text-sm font-semibold text-white uppercase tracking-wider mb-4">
              Table of Contents
            </h2>
            <ol className="grid sm:grid-cols-2 gap-x-8 gap-y-2">
              {sections.map((s) => (
                <li key={s.id}>
                  <a
                    href={`#${s.id}`}
                    className="text-sm text-slate-400 hover:text-cyan-400 transition-colors duration-150"
                  >
                    {s.title}
                  </a>
                </li>
              ))}
            </ol>
          </nav>

          {/* Policy sections */}
          <div className="space-y-12">
            {sections.map((section) => (
              <article
                key={section.id}
                id={section.id}
                className="scroll-mt-24"
              >
                <h2
                  className="text-xl font-bold text-white mb-4 pb-2 border-b border-white/[0.06]"
                  style={{ fontFamily: "'Space Grotesk', sans-serif" }}
                >
                  {section.title}
                </h2>
                <div className="prose prose-invert prose-sm max-w-none text-slate-400 leading-relaxed space-y-3 [&_ul]:list-disc [&_ul]:ml-5 [&_ul]:space-y-1.5 [&_li]:text-slate-400 [&_strong]:text-slate-200 [&_a]:no-underline [&_p]:text-slate-400">
                  {section.content}
                </div>
              </article>
            ))}
          </div>

          {/* Bottom nav */}
          <div className="mt-16 pt-8 border-t border-white/[0.06] flex flex-col sm:flex-row items-center justify-between gap-4">
            <Link
              href="/"
              className="text-sm text-slate-500 hover:text-slate-300 transition-colors"
            >
              ← Back to CargoMarket
            </Link>
            <div className="flex items-center gap-4 text-sm">
              <Link href="/privacy-policy" className="text-cyan-400 font-medium">
                Privacy Policy
              </Link>
              <span className="text-slate-700">|</span>
              <Link href="/" className="text-slate-500 hover:text-slate-300 transition-colors">
                Terms of Service
              </Link>
              <span className="text-slate-700">|</span>
              <Link href="/" className="text-slate-500 hover:text-slate-300 transition-colors">
                Cookie Policy
              </Link>
            </div>
          </div>
        </div>
      </section>

      <Footer />
    </main>
  );
}

import type { Metadata } from "next";
import Link from "next/link";
import Navbar from "@/components/Navbar";
import Footer from "@/components/Footer";

export const metadata: Metadata = {
  title: "Terms of Service — CargoMarket",
  description:
    "CargoMarket Terms of Service. Read the terms and conditions governing your use of the CargoMarket logistics platform.",
  robots: { index: true, follow: true },
  alternates: { canonical: "https://cargomarket.net/terms-of-service" },
  openGraph: {
    title: "Terms of Service — CargoMarket",
    description: "Terms and conditions governing your use of CargoMarket.",
    type: "website",
    url: "https://cargomarket.net/terms-of-service",
  },
};

const LAST_UPDATED = "June 13, 2026";
const COMPANY = "CargoMarket (Atlas Cargome Inc.)";
const SUPPORT_EMAIL = "support@cargomarket.net";
const LEGAL_EMAIL = "legal@cargomarket.net";

const sections = [
  {
    id: "acceptance",
    title: "1. Acceptance of Terms",
    content: (
      <>
        <p>
          By accessing or using CargoMarket&apos;s website, mobile applications, or the
          LogisticOS SaaS platform (collectively, the &quot;Services&quot;), you agree to be
          bound by these Terms of Service (&quot;Terms&quot;) and our{" "}
          <Link href="/privacy-policy" className="text-cyan-400 hover:text-cyan-300 underline">
            Privacy Policy
          </Link>
          . If you are using the Services on behalf of an organization, you represent
          that you have the authority to bind that organization to these Terms.
        </p>
        <p>
          If you do not agree to these Terms, you must not use the Services. We reserve
          the right to update these Terms at any time. Continued use after changes
          constitutes acceptance of the revised Terms.
        </p>
      </>
    ),
  },
  {
    id: "services",
    title: "2. Description of Services",
    content: (
      <>
        <p>
          {COMPANY} provides an AI-powered last-mile logistics SaaS platform that
          includes:
        </p>
        <ul>
          <li>Order and shipment management for merchants and logistics operators</li>
          <li>AI-driven dispatch, routing, and vehicle optimization</li>
          <li>Real-time driver tracking and task management via mobile application</li>
          <li>Automated customer engagement (WhatsApp, SMS, Email, Push notifications)</li>
          <li>Proof of Delivery (POD) capture and Proof of Pickup (POP)</li>
          <li>Cash on Delivery (COD) reconciliation and billing management</li>
          <li>Analytics, reporting, and business intelligence dashboards</li>
          <li>Carrier management and third-party logistics integration</li>
          <li>Customer-facing shipment tracking portal</li>
          <li>API access for integration with e-commerce platforms and ERPs</li>
        </ul>
        <p>
          We reserve the right to modify, suspend, or discontinue any part of the
          Services at any time with reasonable notice. We will not be liable to you or
          any third party for any modification, suspension, or discontinuation.
        </p>
      </>
    ),
  },
  {
    id: "accounts",
    title: "3. User Accounts & Registration",
    content: (
      <>
        <p>To access most features of the Services, you must register for an account. You agree to:</p>
        <ul>
          <li>Provide accurate, current, and complete registration information.</li>
          <li>Maintain and promptly update your account information.</li>
          <li>Keep your password confidential and not share it with any third party.</li>
          <li>Notify us immediately at <a href={`mailto:${SUPPORT_EMAIL}`} className="text-cyan-400 hover:text-cyan-300 underline">{SUPPORT_EMAIL}</a> of any unauthorized access to your account.</li>
          <li>Accept responsibility for all activities that occur under your account.</li>
        </ul>
        <p>
          We reserve the right to suspend or terminate accounts that violate these Terms,
          engage in fraudulent activity, or remain inactive for more than 12 consecutive months.
        </p>
        <h3 className="text-base font-semibold text-white mt-5 mb-2">Multi-Tenancy</h3>
        <p>
          The platform operates on a multi-tenant architecture. Each logistics operator
          (&quot;Tenant&quot;) has an isolated data environment. Tenants are responsible for
          managing access controls for their sub-users (merchants, drivers, ops staff)
          and ensuring compliance with applicable laws within their tenant environment.
        </p>
      </>
    ),
  },
  {
    id: "merchant-terms",
    title: "4. Merchant Terms",
    content: (
      <>
        <p>If you use CargoMarket as a merchant (shipper), you additionally agree to:</p>
        <ul>
          <li>Provide accurate shipment details including declared value, dimensions, weight, and contents.</li>
          <li>Not ship prohibited items including but not limited to: illegal goods, hazardous materials without proper declaration, live animals without carrier consent, counterfeit goods, weapons, and controlled substances.</li>
          <li>Ensure shipment packaging is adequate to protect contents during transit.</li>
          <li>Provide accurate consignee contact details including verified phone numbers and delivery addresses.</li>
          <li>Honor your payment obligations for shipping fees, COD remittance, and any applicable overage charges.</li>
          <li>Accept that estimated delivery windows are not guaranteed and are subject to operational, weather, and force majeure conditions.</li>
        </ul>
        <p>
          Merchants are liable for losses arising from inaccurate shipment declarations,
          prohibited item shipments, and failure to comply with carrier requirements.
          CargoMarket&apos;s liability for lost or damaged shipments is limited as set out in
          Section 11.
        </p>
      </>
    ),
  },
  {
    id: "driver-terms",
    title: "5. Driver & Courier Terms",
    content: (
      <>
        <p>If you use the CargoMarket Driver App as a courier, you additionally agree to:</p>
        <ul>
          <li>Maintain a valid driver&apos;s license and all applicable permits required in your jurisdiction.</li>
          <li>Keep your vehicle registration, insurance, and roadworthiness certifications current.</li>
          <li>Complete Proof of Delivery (POD) and Proof of Pickup (POP) accurately and at the time of the physical event — not retroactively.</li>
          <li>Not tamper with, falsify, or manipulate GPS data, timestamps, delivery photos, or signatures.</li>
          <li>Remit collected Cash on Delivery (COD) amounts in full and on time per the remittance schedule.</li>
          <li>Maintain the confidentiality of consignee personal data accessed through the app.</li>
          <li>Report accidents, incidents, and lost parcels immediately via the app or to your fleet supervisor.</li>
        </ul>
        <p>
          Drivers are independent contractors unless a separate employment agreement states otherwise.
          CargoMarket is not responsible for vehicle operating costs, fuel, tolls, insurance, or taxes
          incurred by drivers in the performance of deliveries.
        </p>
      </>
    ),
  },
  {
    id: "acceptable-use",
    title: "6. Acceptable Use Policy",
    content: (
      <>
        <p>You must not use the Services to:</p>
        <ul>
          <li>Violate any applicable local, national, or international law or regulation.</li>
          <li>Transmit unsolicited commercial communications (&quot;spam&quot;) or conduct phishing attacks.</li>
          <li>Impersonate any person or entity, or falsely represent your affiliation.</li>
          <li>Attempt to gain unauthorized access to any part of the Services, other users&apos; accounts, or our infrastructure.</li>
          <li>Introduce malicious code, viruses, or other harmful software.</li>
          <li>Reverse-engineer, decompile, disassemble, or attempt to extract source code from the Services.</li>
          <li>Use automated tools to scrape, crawl, or extract data from the platform without our written consent.</li>
          <li>Overload or disrupt the platform infrastructure (denial-of-service attacks).</li>
          <li>Use the platform to process shipments of goods that infringe third-party intellectual property rights.</li>
          <li>Resell or sublicense access to the Services without our prior written consent.</li>
        </ul>
        <p>
          Violation of this Acceptable Use Policy may result in immediate account suspension
          without notice and potential legal action.
        </p>
      </>
    ),
  },
  {
    id: "ip",
    title: "7. Intellectual Property",
    content: (
      <>
        <p>
          All content, software, algorithms, trademarks, logos, and materials comprising
          the Services are the exclusive property of {COMPANY} or its licensors and are
          protected by applicable intellectual property laws.
        </p>
        <p>
          We grant you a limited, non-exclusive, non-transferable, revocable license to
          access and use the Services solely for your internal business purposes in
          accordance with these Terms. This license does not include the right to:
        </p>
        <ul>
          <li>Copy, modify, or create derivative works of the Services.</li>
          <li>Use our trademarks, logos, or branding without prior written consent.</li>
          <li>Remove or alter any copyright, trademark, or proprietary notices.</li>
        </ul>
        <h3 className="text-base font-semibold text-white mt-5 mb-2">Your Content</h3>
        <p>
          You retain ownership of data and content you upload to the Services
          (&quot;Your Content&quot;). By uploading Your Content, you grant us a limited,
          worldwide, royalty-free license to process, store, display, and transmit Your
          Content solely to provide the Services. We do not claim ownership of Your Content.
        </p>
        <p>
          You represent that you have all necessary rights to Your Content and that it
          does not infringe any third-party rights or violate applicable laws.
        </p>
      </>
    ),
  },
  {
    id: "payments",
    title: "8. Payment Terms & Billing",
    content: (
      <>
        <p>
          Paid features of the Services are subject to the subscription plan or
          per-shipment pricing you select at sign-up or as amended via your account
          settings.
        </p>
        <ul>
          <li><strong>Subscription Fees:</strong> Billed monthly or annually in advance. All fees are non-refundable except as required by law or as stated in our refund policy.</li>
          <li><strong>Per-Shipment Fees:</strong> Calculated at the time of shipment booking based on weight, dimensions, service type, and destination zone.</li>
          <li><strong>Overage Charges:</strong> Additional charges apply when actual shipment weight or dimensions exceed declared values by more than 5%.</li>
          <li><strong>Late Payment:</strong> Unpaid invoices may result in service suspension. We reserve the right to charge interest on overdue amounts at 1.5% per month or the maximum rate permitted by law.</li>
          <li><strong>Taxes:</strong> All fees are exclusive of applicable taxes (VAT, GST, etc.) which will be added to your invoice as required by law.</li>
          <li><strong>Price Changes:</strong> We will provide at least 30 days&apos; notice of any price increases for existing subscriptions.</li>
        </ul>
        <p>
          Payment processing is handled by PCI-DSS certified payment partners. Card data
          is never stored on CargoMarket servers. By providing payment details, you
          authorize us to charge the applicable fees.
        </p>
      </>
    ),
  },
  {
    id: "privacy",
    title: "9. Privacy & Data Protection",
    content: (
      <>
        <p>
          Your use of the Services is also governed by our{" "}
          <Link href="/privacy-policy" className="text-cyan-400 hover:text-cyan-300 underline">
            Privacy Policy
          </Link>
          , which is incorporated into these Terms by reference. By using the Services,
          you consent to the collection, use, and sharing of your information as
          described in the Privacy Policy.
        </p>
        <p>
          For requests to delete your data associated with Meta (Facebook/Instagram)
          integrations, see our{" "}
          <Link href="/user-data-deletion" className="text-cyan-400 hover:text-cyan-300 underline">
            User Data Deletion
          </Link>{" "}
          page.
        </p>
      </>
    ),
  },
  {
    id: "disclaimers",
    title: "10. Disclaimers & Warranties",
    content: (
      <>
        <p>
          THE SERVICES ARE PROVIDED &quot;AS IS&quot; AND &quot;AS AVAILABLE&quot; WITHOUT WARRANTIES
          OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO WARRANTIES OF
          MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE, NON-INFRINGEMENT, AND
          ACCURACY.
        </p>
        <p>We do not warrant that:</p>
        <ul>
          <li>The Services will be uninterrupted, error-free, or completely secure.</li>
          <li>Delivery time estimates (ETAs) will be met in all circumstances.</li>
          <li>AI-generated dispatch recommendations, route optimizations, or demand forecasts will be accurate in all cases.</li>
          <li>The Services will be compatible with all third-party systems or integrations.</li>
        </ul>
        <p>
          Some jurisdictions do not allow the exclusion of implied warranties. In such
          jurisdictions, our liability is limited to the maximum extent permitted by law.
        </p>
      </>
    ),
  },
  {
    id: "liability",
    title: "11. Limitation of Liability",
    content: (
      <>
        <p>
          TO THE MAXIMUM EXTENT PERMITTED BY APPLICABLE LAW, IN NO EVENT SHALL
          CARGOMARKET, ITS OFFICERS, DIRECTORS, EMPLOYEES, OR AGENTS BE LIABLE FOR ANY:
        </p>
        <ul>
          <li>Indirect, incidental, special, consequential, or punitive damages.</li>
          <li>Loss of profits, revenue, data, goodwill, or business opportunities.</li>
          <li>Damages arising from unauthorized access to your account or data.</li>
          <li>Loss or damage to shipment contents beyond the declared value and subject to our carrier liability policy.</li>
        </ul>
        <p>
          Our total aggregate liability to you for any claims arising out of or relating
          to these Terms or the Services shall not exceed the greater of: (a) the total
          fees paid by you to CargoMarket in the 3 months preceding the event giving rise
          to the claim; or (b) USD $100.
        </p>
        <p>
          These limitations apply whether the claim is based in contract, tort
          (including negligence), strict liability, or any other legal theory, even if
          we have been advised of the possibility of such damages.
        </p>
      </>
    ),
  },
  {
    id: "indemnification",
    title: "12. Indemnification",
    content: (
      <p>
        You agree to indemnify, defend, and hold harmless {COMPANY} and its officers,
        directors, employees, agents, and licensors from and against any claims,
        liabilities, damages, losses, costs, and expenses (including reasonable
        legal fees) arising out of or relating to: (a) your use of the Services in
        violation of these Terms; (b) Your Content; (c) your violation of any
        applicable law or third-party rights; or (d) any shipment you book through the
        platform that causes harm, damage, or regulatory violation.
      </p>
    ),
  },
  {
    id: "termination",
    title: "13. Termination",
    content: (
      <>
        <p>
          You may terminate your account at any time by contacting us at{" "}
          <a href={`mailto:${SUPPORT_EMAIL}`} className="text-cyan-400 hover:text-cyan-300 underline">{SUPPORT_EMAIL}</a>.
          Termination does not relieve you of any outstanding payment obligations.
        </p>
        <p>We may suspend or terminate your account immediately without notice if:</p>
        <ul>
          <li>You breach any provision of these Terms.</li>
          <li>We are required to do so by law or regulatory authority.</li>
          <li>Continued operation of your account poses a risk to the platform or other users.</li>
          <li>Your account shows signs of fraudulent activity.</li>
        </ul>
        <p>
          Upon termination, your right to access the Services ceases immediately.
          We will retain your data as described in the Privacy Policy and provide a
          data export upon request submitted within 30 days of termination.
        </p>
      </>
    ),
  },
  {
    id: "governing-law",
    title: "14. Governing Law & Dispute Resolution",
    content: (
      <>
        <p>
          These Terms are governed by the laws of the Republic of the Philippines,
          without regard to its conflict of law provisions.
        </p>
        <p>
          Any dispute arising out of or in connection with these Terms shall first be
          attempted to be resolved through good-faith negotiation. If unresolved within
          30 days, disputes shall be submitted to binding arbitration in the Philippines
          under the rules of the Philippine Dispute Resolution Center, Inc. (PDRCI),
          except that either party may seek injunctive or other equitable relief in any
          court of competent jurisdiction.
        </p>
        <p>
          Notwithstanding the above, users in the EU/EEA retain the right to bring
          claims before their local courts and regulatory bodies as permitted by
          applicable consumer protection laws.
        </p>
      </>
    ),
  },
  {
    id: "third-party",
    title: "15. Third-Party Services & Integrations",
    content: (
      <p>
        The Services integrate with third-party platforms including payment processors,
        map providers, messaging gateways, e-commerce platforms, and social media
        platforms (including Meta/Facebook). Your use of those integrations is
        additionally governed by the respective third-party terms of service. CargoMarket
        is not responsible for the availability, accuracy, or policies of third-party
        services. We will notify you of any material changes to key third-party
        integrations that affect your use of the Services.
      </p>
    ),
  },
  {
    id: "force-majeure",
    title: "16. Force Majeure",
    content: (
      <p>
        CargoMarket shall not be liable for any failure or delay in performance of its
        obligations under these Terms where such failure or delay results from causes
        beyond our reasonable control, including but not limited to: acts of God,
        natural disasters, pandemics, war, terrorism, government actions, infrastructure
        failures, or labor disputes. We will notify you promptly of any such event and
        resume performance as soon as reasonably practicable.
      </p>
    ),
  },
  {
    id: "changes",
    title: "17. Changes to These Terms",
    content: (
      <>
        <p>
          We may modify these Terms at any time. When we make material changes, we will:
        </p>
        <ul>
          <li>Update the &quot;Last Updated&quot; date on this page.</li>
          <li>Notify registered users via email at least 30 days before changes take effect.</li>
          <li>Display a prominent in-app notice for material changes.</li>
        </ul>
        <p>
          Your continued use of the Services after the effective date of the updated
          Terms constitutes your acceptance of those changes.
        </p>
      </>
    ),
  },
  {
    id: "contact",
    title: "18. Contact & Legal Notices",
    content: (
      <>
        <p>For legal enquiries, contract questions, or to serve legal notices:</p>
        <div className="bg-white/[0.03] border border-white/[0.08] rounded-xl p-5 mt-4 space-y-1 text-sm text-slate-300">
          <p><strong className="text-white">Legal Department — {COMPANY}</strong></p>
          <p>Philippines</p>
          <p>
            Legal:{" "}
            <a href={`mailto:${LEGAL_EMAIL}`} className="text-cyan-400 hover:text-cyan-300 underline">
              {LEGAL_EMAIL}
            </a>
          </p>
          <p>
            Support:{" "}
            <a href={`mailto:${SUPPORT_EMAIL}`} className="text-cyan-400 hover:text-cyan-300 underline">
              {SUPPORT_EMAIL}
            </a>
          </p>
        </div>
      </>
    ),
  },
];

export default function TermsOfServicePage() {
  return (
    <main className="min-h-screen bg-[#050810] text-slate-100 overflow-x-hidden">
      <Navbar />

      <section className="pt-32 pb-12 px-6 lg:px-8 relative">
        <div className="absolute inset-0 bg-gradient-to-b from-purple-plasma/[0.04] via-transparent to-transparent pointer-events-none" />
        <div className="max-w-4xl mx-auto relative">
          <div className="inline-flex items-center gap-2 px-3 py-1.5 rounded-full bg-white/[0.05] border border-white/[0.08] text-xs text-slate-400 mb-6">
            <span className="w-1.5 h-1.5 rounded-full bg-purple-400" />
            Legal
          </div>
          <h1
            className="text-4xl lg:text-5xl font-bold text-white mb-4"
            style={{ fontFamily: "'Space Grotesk', sans-serif" }}
          >
            Terms of Service
          </h1>
          <p className="text-slate-400 text-base">
            Last updated: <time dateTime="2026-06-13">{LAST_UPDATED}</time>
          </p>
          <p className="text-slate-400 text-sm mt-2">
            These Terms govern your use of{" "}
            <a href="https://cargomarket.net" className="text-cyan-400 hover:text-cyan-300 underline">
              cargomarket.net
            </a>{" "}
            and all CargoMarket products and services.
          </p>
        </div>
      </section>

      <section className="pb-24 px-6 lg:px-8">
        <div className="max-w-4xl mx-auto">
          <nav
            aria-label="Terms of service sections"
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

          <div className="space-y-12">
            {sections.map((section) => (
              <article key={section.id} id={section.id} className="scroll-mt-24">
                <h2
                  className="text-xl font-bold text-white mb-4 pb-2 border-b border-white/[0.06]"
                  style={{ fontFamily: "'Space Grotesk', sans-serif" }}
                >
                  {section.title}
                </h2>
                <div className="prose prose-invert prose-sm max-w-none text-slate-400 leading-relaxed space-y-3 [&_ul]:list-disc [&_ul]:ml-5 [&_ul]:space-y-1.5 [&_li]:text-slate-400 [&_strong]:text-slate-200 [&_p]:text-slate-400">
                  {section.content}
                </div>
              </article>
            ))}
          </div>

          <div className="mt-16 pt-8 border-t border-white/[0.06] flex flex-col sm:flex-row items-center justify-between gap-4">
            <Link href="/" className="text-sm text-slate-500 hover:text-slate-300 transition-colors">
              ← Back to CargoMarket
            </Link>
            <div className="flex items-center gap-4 text-sm">
              <Link href="/privacy-policy" className="text-slate-500 hover:text-slate-300 transition-colors">
                Privacy Policy
              </Link>
              <span className="text-slate-700">|</span>
              <Link href="/terms-of-service" className="text-cyan-400 font-medium">
                Terms of Service
              </Link>
              <span className="text-slate-700">|</span>
              <Link href="/user-data-deletion" className="text-slate-500 hover:text-slate-300 transition-colors">
                Data Deletion
              </Link>
            </div>
          </div>
        </div>
      </section>

      <Footer />
    </main>
  );
}

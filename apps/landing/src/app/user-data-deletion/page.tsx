import type { Metadata } from "next";
import Link from "next/link";
import Navbar from "@/components/Navbar";
import Footer from "@/components/Footer";

export const metadata: Metadata = {
  title: "User Data Deletion — CargoMarket",
  description:
    "Request deletion of your personal data collected by CargoMarket through Meta (Facebook/Instagram) integrations. Instructions and confirmation status.",
  robots: { index: true, follow: true },
  alternates: { canonical: "https://cargomarket.net/user-data-deletion" },
  openGraph: {
    title: "User Data Deletion — CargoMarket",
    description:
      "How to request deletion of your data from CargoMarket's Meta (Facebook/Instagram) integration.",
    type: "website",
    url: "https://cargomarket.net/user-data-deletion",
  },
};

const DELETION_EMAIL = "privacy@cargomarket.net";
const RESPONSE_DAYS = 30;

const steps = [
  {
    number: "01",
    title: "Submit your request",
    body: `Email ${DELETION_EMAIL} with subject "Data Deletion Request — Meta". Include the Facebook/Instagram account name or email address linked to your CargoMarket account.`,
  },
  {
    number: "02",
    title: "Identity verification",
    body: "We will verify your identity within 5 business days and send you a confirmation email with a unique deletion confirmation code.",
  },
  {
    number: "03",
    title: "Data deletion executed",
    body: `Within ${RESPONSE_DAYS} days of your verified request, we will delete or anonymize all personal data collected through the Meta integration from our active systems.`,
  },
  {
    number: "04",
    title: "Deletion confirmation",
    body: "We will send a final confirmation email once deletion is complete. Use your confirmation code at any time to check the status of your request.",
  },
];

const dataItems = [
  { label: "Facebook / Instagram User ID", deleted: true },
  { label: "Name and profile information received from Meta", deleted: true },
  { label: "Email address received from Meta login", deleted: true },
  { label: "Profile photo URL received from Meta", deleted: true },
  { label: "Meta access tokens stored in our systems", deleted: true },
  { label: "Behavioral analytics tied to your Meta identity", deleted: true },
  { label: "Marketing audiences built from your Meta profile", deleted: true },
  {
    label: "Completed shipment transaction records (required for legal / financial compliance)",
    deleted: false,
    note: "Retained for up to 7 years per financial record-keeping law; anonymized after retention period.",
  },
  {
    label: "Proof of Delivery (POD) records for completed shipments",
    deleted: false,
    note: "Retained for 3 years per carrier liability requirements.",
  },
  {
    label: "Anonymized platform analytics not linked to your identity",
    deleted: false,
    note: "Aggregate data that cannot be re-linked to you is not deleted.",
  },
];

export default function UserDataDeletionPage() {
  return (
    <main className="min-h-screen bg-[#050810] text-slate-100 overflow-x-hidden">
      <Navbar />

      {/* Hero */}
      <section className="pt-32 pb-12 px-6 lg:px-8 relative">
        <div className="absolute inset-0 bg-gradient-to-b from-green-signal/[0.03] via-transparent to-transparent pointer-events-none" />
        <div className="max-w-4xl mx-auto relative">
          <div className="inline-flex items-center gap-2 px-3 py-1.5 rounded-full bg-white/[0.05] border border-white/[0.08] text-xs text-slate-400 mb-6">
            <span className="w-1.5 h-1.5 rounded-full bg-green-400" />
            Legal · Meta Platform
          </div>
          <h1
            className="text-4xl lg:text-5xl font-bold text-white mb-4"
            style={{ fontFamily: "'Space Grotesk', sans-serif" }}
          >
            User Data Deletion
          </h1>
          <p className="text-slate-400 text-base max-w-2xl">
            This page describes how CargoMarket processes user data collected through
            Meta (Facebook/Instagram) integrations and how you can request deletion of
            that data as required by the{" "}
            <a
              href="https://developers.facebook.com/policy/"
              target="_blank"
              rel="noopener noreferrer"
              className="text-cyan-400 hover:text-cyan-300 underline"
            >
              Meta Platform Policy
            </a>
            .
          </p>
          <p className="text-slate-500 text-sm mt-3">
            Last updated: <time dateTime="2026-06-13">June 13, 2026</time>
          </p>
        </div>
      </section>

      <section className="pb-24 px-6 lg:px-8">
        <div className="max-w-4xl mx-auto space-y-16">

          {/* What data we collect via Meta */}
          <article id="what-we-collect">
            <h2
              className="text-xl font-bold text-white mb-4 pb-2 border-b border-white/[0.06]"
              style={{ fontFamily: "'Space Grotesk', sans-serif" }}
            >
              1. What Data We Receive from Meta
            </h2>
            <p className="text-slate-400 text-sm leading-relaxed mb-4">
              When you connect your Facebook or Instagram account to CargoMarket (for
              example, to log in via Facebook or to receive shipment notifications via
              Messenger), we may receive the following data from Meta Platforms, Inc.:
            </p>
            <ul className="space-y-2 text-sm text-slate-400 list-disc ml-5">
              <li>Your Facebook or Instagram User ID</li>
              <li>Your name and profile picture URL</li>
              <li>Your email address (if granted permission)</li>
              <li>Access tokens required to send you notifications via Messenger</li>
              <li>Aggregated engagement data from Meta advertising campaigns (not personally identifiable)</li>
            </ul>
            <p className="text-slate-400 text-sm leading-relaxed mt-4">
              We do not receive or store your Facebook password, payment details, or any
              private messages. For full details on how we use this data, see our{" "}
              <Link href="/privacy-policy#cookies" className="text-cyan-400 hover:text-cyan-300 underline">
                Privacy Policy — Section 6
              </Link>
              .
            </p>
          </article>

          {/* How to request deletion */}
          <article id="how-to-delete">
            <h2
              className="text-xl font-bold text-white mb-6 pb-2 border-b border-white/[0.06]"
              style={{ fontFamily: "'Space Grotesk', sans-serif" }}
            >
              2. How to Request Deletion
            </h2>
            <p className="text-slate-400 text-sm leading-relaxed mb-8">
              You can request deletion of all personal data we collected through Meta at
              any time. There are two ways to submit your request:
            </p>

            {/* CTA card */}
            <div className="bg-gradient-to-r from-cyan-neon/[0.08] to-purple-plasma/[0.08] border border-cyan-neon/20 rounded-2xl p-6 mb-8">
              <h3 className="text-base font-semibold text-white mb-2">
                Option A — Email Request (Recommended)
              </h3>
              <p className="text-slate-400 text-sm mb-4">
                Send a deletion request directly to our Privacy team:
              </p>
              <a
                href={`mailto:${DELETION_EMAIL}?subject=Data%20Deletion%20Request%20%E2%80%94%20Meta&body=Hello%2C%0A%0AI%20would%20like%20to%20request%20deletion%20of%20all%20personal%20data%20collected%20through%20my%20Meta%20(Facebook%2FInstagram)%20account%20connection%20with%20CargoMarket.%0A%0AFacebook%2FInstagram%20account%3A%20%5Benter%20your%20account%20name%20or%20email%5D%0ACargoMarket%20account%20email%3A%20%5Benter%20your%20CargoMarket%20email%5D%0A%0AThank%20you.`}
                className="inline-flex items-center gap-2 px-5 py-2.5 rounded-xl text-sm font-semibold bg-gradient-to-r from-cyan-neon to-purple-plasma text-[#050810] hover:shadow-glow-cyan transition-all duration-300"
              >
                Email {DELETION_EMAIL}
              </a>
            </div>

            <div className="bg-white/[0.03] border border-white/[0.08] rounded-2xl p-6">
              <h3 className="text-base font-semibold text-white mb-2">
                Option B — Via Facebook Settings
              </h3>
              <p className="text-slate-400 text-sm mb-3">
                You can also revoke CargoMarket&apos;s access and request data deletion
                directly through your Facebook account:
              </p>
              <ol className="space-y-1.5 text-sm text-slate-400 list-decimal ml-5">
                <li>Go to your Facebook <strong className="text-slate-200">Settings &amp; Privacy → Settings</strong></li>
                <li>Navigate to <strong className="text-slate-200">Apps and Websites</strong></li>
                <li>Find <strong className="text-slate-200">CargoMarket</strong> in the list and click <strong className="text-slate-200">Remove</strong></li>
                <li>Select <strong className="text-slate-200">&quot;Delete all data from [app] as well&quot;</strong> when prompted</li>
              </ol>
              <p className="text-slate-500 text-xs mt-3">
                Removing the app via Facebook triggers our automated Data Deletion
                Callback which queues your data for deletion within {RESPONSE_DAYS} days.
              </p>
            </div>
          </article>

          {/* Step by step */}
          <article id="process">
            <h2
              className="text-xl font-bold text-white mb-6 pb-2 border-b border-white/[0.06]"
              style={{ fontFamily: "'Space Grotesk', sans-serif" }}
            >
              3. Deletion Process
            </h2>
            <div className="grid sm:grid-cols-2 gap-4">
              {steps.map((step) => (
                <div
                  key={step.number}
                  className="bg-white/[0.03] border border-white/[0.08] rounded-xl p-5 relative overflow-hidden"
                >
                  <span
                    className="text-5xl font-black text-white/[0.04] absolute top-2 right-4 select-none"
                    style={{ fontFamily: "'JetBrains Mono', monospace" }}
                  >
                    {step.number}
                  </span>
                  <p
                    className="text-xs font-semibold text-cyan-400 uppercase tracking-wider mb-1"
                    style={{ fontFamily: "'JetBrains Mono', monospace" }}
                  >
                    Step {step.number}
                  </p>
                  <h3 className="text-sm font-semibold text-white mb-2">{step.title}</h3>
                  <p className="text-xs text-slate-400 leading-relaxed">{step.body}</p>
                </div>
              ))}
            </div>
          </article>

          {/* What gets deleted */}
          <article id="what-gets-deleted">
            <h2
              className="text-xl font-bold text-white mb-4 pb-2 border-b border-white/[0.06]"
              style={{ fontFamily: "'Space Grotesk', sans-serif" }}
            >
              4. What Gets Deleted vs. Retained
            </h2>
            <p className="text-slate-400 text-sm leading-relaxed mb-6">
              Upon a verified deletion request, we delete all personal data associated
              with your Meta integration. Some data must be retained to comply with legal
              and regulatory obligations.
            </p>
            <div className="space-y-2">
              {dataItems.map((item, i) => (
                <div
                  key={i}
                  className="flex items-start gap-3 p-4 rounded-xl bg-white/[0.02] border border-white/[0.05]"
                >
                  <span
                    className={`mt-0.5 flex-shrink-0 w-5 h-5 rounded-full flex items-center justify-center text-xs font-bold ${
                      item.deleted
                        ? "bg-green-signal/20 text-green-400"
                        : "bg-amber-400/20 text-amber-400"
                    }`}
                  >
                    {item.deleted ? "✓" : "~"}
                  </span>
                  <div>
                    <p className="text-sm text-slate-300">{item.label}</p>
                    {item.note && (
                      <p className="text-xs text-slate-500 mt-0.5">{item.note}</p>
                    )}
                  </div>
                  <span
                    className={`ml-auto flex-shrink-0 text-xs font-medium px-2 py-0.5 rounded-full ${
                      item.deleted
                        ? "bg-green-signal/10 text-green-400"
                        : "bg-amber-400/10 text-amber-400"
                    }`}
                  >
                    {item.deleted ? "Deleted" : "Retained"}
                  </span>
                </div>
              ))}
            </div>
          </article>

          {/* Check status */}
          <article id="check-status">
            <h2
              className="text-xl font-bold text-white mb-4 pb-2 border-b border-white/[0.06]"
              style={{ fontFamily: "'Space Grotesk', sans-serif" }}
            >
              5. Check Deletion Request Status
            </h2>
            <p className="text-slate-400 text-sm leading-relaxed mb-4">
              After submitting a deletion request, you will receive a unique confirmation
              code via email. To check the current status of your request, email{" "}
              <a
                href={`mailto:${DELETION_EMAIL}?subject=Deletion%20Status%20Check&body=My%20confirmation%20code%20is%3A%20`}
                className="text-cyan-400 hover:text-cyan-300 underline"
              >
                {DELETION_EMAIL}
              </a>{" "}
              with your confirmation code and subject line{" "}
              <span
                className="font-mono text-xs bg-white/[0.07] px-2 py-0.5 rounded text-slate-300"
                style={{ fontFamily: "'JetBrains Mono', monospace" }}
              >
                Deletion Status Check
              </span>
              .
            </p>
            <div className="bg-white/[0.03] border border-white/[0.08] rounded-xl p-5 text-sm">
              <p className="text-slate-300 font-medium mb-1">Status definitions:</p>
              <ul className="space-y-1.5 text-slate-400">
                <li><span className="text-yellow-400 font-mono text-xs">PENDING</span> — Request received; identity verification in progress.</li>
                <li><span className="text-cyan-400 font-mono text-xs">VERIFIED</span> — Identity confirmed; deletion queued within {RESPONSE_DAYS} days.</li>
                <li><span className="text-green-400 font-mono text-xs">COMPLETED</span> — All eligible data has been deleted from active systems.</li>
                <li><span className="text-red-400 font-mono text-xs">REJECTED</span> — Request could not be verified. Contact us for assistance.</li>
              </ul>
            </div>
          </article>

          {/* Contact */}
          <article id="contact">
            <h2
              className="text-xl font-bold text-white mb-4 pb-2 border-b border-white/[0.06]"
              style={{ fontFamily: "'Space Grotesk', sans-serif" }}
            >
              6. Contact
            </h2>
            <div className="bg-white/[0.03] border border-white/[0.08] rounded-xl p-5 text-sm text-slate-300 space-y-1">
              <p><strong className="text-white">Data Privacy Officer — CargoMarket (Atlas Cargome Inc.)</strong></p>
              <p>Philippines</p>
              <p>
                Privacy:{" "}
                <a href={`mailto:${DELETION_EMAIL}`} className="text-cyan-400 hover:text-cyan-300 underline">
                  {DELETION_EMAIL}
                </a>
              </p>
              <p>
                Support:{" "}
                <a href="mailto:support@cargomarket.net" className="text-cyan-400 hover:text-cyan-300 underline">
                  support@cargomarket.net
                </a>
              </p>
            </div>
            <p className="text-slate-500 text-xs mt-4">
              This page satisfies the Meta Platform Policy requirement for a User Data
              Deletion Instructions URL. Automated deletion callbacks are handled via the
              API endpoint at{" "}
              <span
                className="font-mono bg-white/[0.07] px-1.5 py-0.5 rounded text-slate-400"
                style={{ fontFamily: "'JetBrains Mono', monospace" }}
              >
                POST /api/user-data-deletion
              </span>
              .
            </p>
          </article>

          {/* Bottom nav */}
          <div className="pt-8 border-t border-white/[0.06] flex flex-col sm:flex-row items-center justify-between gap-4">
            <Link href="/" className="text-sm text-slate-500 hover:text-slate-300 transition-colors">
              ← Back to CargoMarket
            </Link>
            <div className="flex items-center gap-4 text-sm">
              <Link href="/privacy-policy" className="text-slate-500 hover:text-slate-300 transition-colors">
                Privacy Policy
              </Link>
              <span className="text-slate-700">|</span>
              <Link href="/terms-of-service" className="text-slate-500 hover:text-slate-300 transition-colors">
                Terms of Service
              </Link>
              <span className="text-slate-700">|</span>
              <Link href="/user-data-deletion" className="text-cyan-400 font-medium">
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

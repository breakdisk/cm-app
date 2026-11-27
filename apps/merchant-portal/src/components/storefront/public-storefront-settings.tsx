"use client";
/**
 * Claiming a public storefront: the link name, the custom domain, and the
 * switch that puts the menu on the open internet.
 *
 * The vendor's Storefront console is where a catalog is curated. This is where
 * it gets a customer-facing address — the link for an Instagram bio, a takeaway
 * counter, or a domain the vendor already owns.
 *
 * **Publishing is opt-in and off by default.** A catalog and its prices going
 * public is a decision, and the switch says what it does rather than reading as
 * a settings toggle.
 */
import { useEffect, useState } from "react";
import { Check, Copy, Globe, Loader2, Link2 } from "lucide-react";

import { authFetch } from "@/lib/auth/auth-fetch";
import { API_BASE } from "@/lib/api/endpoints";

interface Settings {
  slug: string | null;
  custom_domain: string | null;
  tagline: string | null;
  public_enabled: boolean;
  /** Built server-side — the portal does not know the public origin. */
  public_url: string | null;
}

const FIELD =
  "w-full rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white placeholder:text-white/30 outline-none transition focus:border-cyan-neon/50 focus:bg-white/10";

export function PublicStorefrontSettings() {
  const [s, setS] = useState<Settings | null>(null);
  const [slug, setSlug] = useState("");
  const [domain, setDomain] = useState("");
  const [tagline, setTagline] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const load = async () => {
    try {
      const res = await authFetch(`${API_BASE}/v1/omnideliv/vendors/me/storefront`);
      if (!res.ok) return;
      const data = (await res.json()) as Settings;
      setS(data);
      setSlug(data.slug ?? "");
      setDomain(data.custom_domain ?? "");
      setTagline(data.tagline ?? "");
    } catch {
      // Not fatal — the rest of the Storefront page still works.
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const save = async (patch: Partial<Record<string, unknown>>) => {
    setSaving(true);
    setError(null);
    try {
      const res = await authFetch(`${API_BASE}/v1/omnideliv/vendors/me/storefront`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(patch),
      });
      if (!res.ok) {
        // The server owns these rules — a taken slug, a reserved domain, a bad
        // shape — and its wording is more precise than anything guessed here.
        throw new Error((await res.text().catch(() => "")) || "Could not save");
      }
      const data = (await res.json()) as Settings;
      setS(data);
      setSlug(data.slug ?? "");
      setDomain(data.custom_domain ?? "");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not save");
    } finally {
      setSaving(false);
    }
  };

  if (!s) return null;

  const copyUrl = async () => {
    if (!s.public_url) return;
    try {
      await navigator.clipboard.writeText(s.public_url);
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    } catch {
      setError("Your browser blocked clipboard access.");
    }
  };

  return (
    <section className="space-y-4 rounded-xl border border-white/10 bg-white/[0.03] p-4 backdrop-blur-xl sm:p-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="font-heading text-lg font-semibold text-white">
            Public menu page
          </h2>
          <p className="mt-1 text-sm text-white/50">
            A link anyone can open — no app, no account. Put it in a bio, on a
            receipt, or on your own domain.
          </p>
        </div>
        <button
          onClick={() => void save({ public_enabled: !s.public_enabled })}
          disabled={saving}
          className={
            s.public_enabled
              ? "inline-flex shrink-0 items-center gap-2 rounded-lg border border-green-signal/40 bg-green-signal/10 px-4 py-2 text-sm font-medium text-green-signal transition hover:bg-green-signal/20 disabled:opacity-40"
              : "inline-flex shrink-0 items-center gap-2 rounded-lg border border-white/10 bg-white/5 px-4 py-2 text-sm text-white/70 transition hover:bg-white/10 disabled:opacity-40"
          }
        >
          {saving ? <Loader2 className="h-4 w-4 animate-spin" /> : <Globe className="h-4 w-4" />}
          {s.public_enabled ? "Published" : "Not published"}
        </button>
      </div>

      {s.public_enabled && s.public_url && (
        <div className="flex flex-wrap items-center gap-2 rounded-lg border border-cyan-neon/25 bg-cyan-neon/[0.06] px-3 py-2">
          <Link2 className="h-4 w-4 shrink-0 text-cyan-neon" />
          <a
            href={s.public_url}
            target="_blank"
            rel="noreferrer"
            className="min-w-0 flex-1 truncate text-sm text-cyan-neon underline-offset-2 hover:underline"
          >
            {s.public_url}
          </a>
          <button
            onClick={copyUrl}
            aria-label="Copy the public link"
            className="shrink-0 rounded-md border border-white/10 bg-white/5 p-1.5 text-white/60 transition hover:bg-white/10"
          >
            {copied ? (
              <Check className="h-3.5 w-3.5 text-green-signal" />
            ) : (
              <Copy className="h-3.5 w-3.5" />
            )}
          </button>
        </div>
      )}

      {error && (
        <p className="rounded-lg border border-red-signal/30 bg-red-signal/10 px-3 py-2 text-sm text-red-signal">
          {error}
        </p>
      )}

      <div className="grid gap-4 sm:grid-cols-2">
        <label className="space-y-1.5">
          <span className="text-xs font-medium uppercase tracking-wide text-white/50">
            Link name
          </span>
          <input
            value={slug}
            onChange={(e) => setSlug(e.target.value)}
            onBlur={() => slug !== (s.slug ?? "") && void save({ slug })}
            placeholder="kanto-freestyle"
            className={FIELD}
          />
          <span className="block text-xs text-white/30">
            Lowercase letters, numbers and hyphens. This becomes part of your
            link, so it is worth getting right before you print it.
          </span>
        </label>

        <label className="space-y-1.5">
          <span className="text-xs font-medium uppercase tracking-wide text-white/50">
            Your own domain (optional)
          </span>
          <input
            value={domain}
            onChange={(e) => setDomain(e.target.value)}
            onBlur={() =>
              domain !== (s.custom_domain ?? "") && void save({ custom_domain: domain })
            }
            placeholder="menu.yourrestaurant.com"
            className={FIELD}
          />
          <span className="block text-xs text-white/30">
            Point a CNAME for this name at our host, then enter it here. It can
            take a little while for DNS to spread.
          </span>
        </label>

        <label className="space-y-1.5 sm:col-span-2">
          <span className="text-xs font-medium uppercase tracking-wide text-white/50">
            Tagline
          </span>
          <input
            value={tagline}
            onChange={(e) => setTagline(e.target.value)}
            onBlur={() => tagline !== (s.tagline ?? "") && void save({ tagline })}
            maxLength={160}
            placeholder="Charcoal-grilled chicken, Malate since 2011"
            className={FIELD}
          />
          <span className="block text-xs text-white/30">
            One line under your name — and the description people see when your
            link is shared on social media.
          </span>
        </label>
      </div>
    </section>
  );
}

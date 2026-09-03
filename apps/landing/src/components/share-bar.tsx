"use client";
/**
 * Sharing a storefront.
 *
 * The only interactive part of an otherwise server-rendered page, kept in its
 * own client component so the menu itself stays server HTML — which is the
 * whole reason a social crawler can read it.
 *
 * Uses the Web Share API where it exists, which on a phone is the native sheet
 * with WhatsApp, Messenger and Instagram already in it. That matters more than
 * a row of per-network buttons: the networks people actually forward menus on
 * are messaging apps, and a hardcoded button list always misses one.
 *
 * The URL is read from the address bar rather than passed in, so a vendor on
 * their own domain shares their own domain — not the platform link that happens
 * to resolve to the same page.
 */
import { useState } from "react";
import { Check, Link2, Share2 } from "lucide-react";

export function ShareBar({ name }: { name: string }) {
  const [copied, setCopied] = useState(false);

  const share = async () => {
    const url = window.location.href;
    if (navigator.share) {
      try {
        await navigator.share({ title: name, url });
        return;
      } catch {
        // Cancelled, or the sheet refused. Fall through to copying, which is
        // never worse than doing nothing.
      }
    }
    await copy();
  };

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(window.location.href);
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    } catch {
      // Clipboard blocked. The URL is in the address bar, which is where
      // someone will look next anyway.
    }
  };

  return (
    <div className="flex flex-wrap gap-2">
      <button
        onClick={share}
        className="inline-flex items-center gap-2 rounded-lg border border-cyan-400/40 bg-cyan-400/10 px-4 py-2 text-sm font-medium text-cyan-300 transition active:scale-[0.98]"
      >
        <Share2 className="h-4 w-4" />
        Share this menu
      </button>
      <button
        onClick={copy}
        aria-label="Copy link"
        className="inline-flex items-center gap-2 rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white/60 transition hover:bg-white/10"
      >
        {copied ? (
          <>
            <Check className="h-4 w-4 text-emerald-400" />
            Copied
          </>
        ) : (
          <>
            <Link2 className="h-4 w-4" />
            Copy link
          </>
        )}
      </button>
    </div>
  );
}

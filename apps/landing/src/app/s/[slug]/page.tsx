/**
 * `/s/:slug` — a vendor's public, shareable storefront.
 *
 * The customer-facing face of the vendor's Storefront console. Same catalog,
 * opposite audience: the console is where a vendor confirms an item is still
 * true, this is where anyone with the link reads the result.
 *
 * ## Why this is a SERVER component
 *
 * A shared link is only worth having if the thing that unfurls it can read it.
 * Facebook, WhatsApp, Slack and X fetch the URL once, with no JavaScript, and
 * take whatever is in the HTML. A client-rendered menu is an empty page to all
 * of them — and to Google. So the fetch happens here, on the server, and
 * `generateMetadata` fills the card from the same response.
 *
 * ## Custom domains
 *
 * `middleware.ts` rewrites the root of any non-platform Host to `/s/<host>`, so
 * a vendor who CNAMEs `menu.kanto.ph` at us lands on this same page. The API
 * resolves a slug and a domain through one lookup, so `slug` here is really
 * "public handle, whichever kind".
 *
 * ## Why there is no basket
 *
 * Ordering needs a principal. The diner page has one — the table session, and
 * it is bounded by having to physically see a printed sticker, plus a per-table
 * session cap and a rate limit. A public link has neither bound: it is the open
 * internet. Minting an anonymous ordering principal from an unbounded public
 * page is a materially bigger security surface than ADR-0017 signed off, so
 * this page browses and shares, and ordering is a deliberate next step.
 */
import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { MapPin, UtensilsCrossed } from "lucide-react";

import { MenuList, type MenuItem } from "@/components/menu-list";
import { ShareBar } from "@/components/share-bar";

/**
 * Server-side only. `NEXT_PUBLIC_*` is inlined at build time; this runs on the
 * server and can read a real runtime variable, so `STOREFRONT_API_URL` lets ops
 * point it at an internal address the browser could never reach. The public
 * fallback is correct in production either way.
 */
const API =
  process.env.STOREFRONT_API_URL ??
  process.env.NEXT_PUBLIC_API_URL ??
  "http://localhost:8000";

interface PublicItem extends MenuItem {
  has_photo: boolean;
}

interface Storefront {
  vendor_id: string;
  tenant_id: string;
  name: string;
  tagline: string | null;
  address: string;
  vertical: string;
  slug: string | null;
  open: boolean;
  items: PublicItem[];
}

/**
 * A shared menu is read far more often than it changes, and a link that goes
 * viral must not turn into a stampede on the catalog. One minute is short
 * enough that a price correction is visible almost immediately.
 */
export const revalidate = 60;

async function load(handle: string): Promise<Storefront | null> {
  try {
    const res = await fetch(
      `${API}/v1/omnideliv/public/storefront/${encodeURIComponent(handle)}`,
      { next: { revalidate } },
    );
    if (!res.ok) return null;
    return (await res.json()) as Storefront;
  } catch {
    // A storefront that cannot be loaded is a 404 rather than a 500: to a
    // stranger following a link the two are the same, and one of them tells
    // them something is broken on our side.
    return null;
  }
}

function photoUrl(s: Storefront, itemId: string): string {
  return `${API}/v1/omnideliv/public/catalog/${s.tenant_id}/items/${itemId}/photo`;
}

export async function generateMetadata({
  params,
}: {
  params: { slug: string };
}): Promise<Metadata> {
  const s = await load(params.slug);
  if (!s) return { title: "Storefront not found" };

  const description =
    s.tagline ??
    `${s.items.length} item${s.items.length === 1 ? "" : "s"} on the menu · ${s.address}`;

  // The first item with a photo becomes the card image. Better than a generic
  // platform logo — someone sharing a restaurant wants their food in the
  // preview — and it costs nothing, because the photo route is already public.
  const hero = s.items.find((i) => i.has_photo);

  return {
    title: s.name,
    description,
    openGraph: {
      type: "website",
      title: s.name,
      description,
      siteName: s.name,
      images: hero ? [{ url: photoUrl(s, hero.item_id), alt: hero.name }] : undefined,
    },
    twitter: {
      card: hero ? "summary_large_image" : "summary",
      title: s.name,
      description,
      images: hero ? [photoUrl(s, hero.item_id)] : undefined,
    },
    // A storefront is meant to be found. The diner page is the opposite and is
    // not indexable — it is a credential in a URL.
    robots: { index: true, follow: true },
  };
}

export default async function StorefrontPage({
  params,
}: {
  params: { slug: string };
}) {
  const s = await load(params.slug);
  if (!s) notFound();

  return (
    <main className="min-h-screen bg-[#050810]">
      <div className="mx-auto max-w-2xl px-4 py-8">
        <header className="space-y-3 border-b border-white/10 pb-6">
          <div className="flex items-start gap-3">
            <span className="rounded-xl border border-white/10 bg-white/5 p-3 text-cyan-400">
              <UtensilsCrossed className="h-6 w-6" />
            </span>
            <div className="min-w-0">
              <h1 className="font-heading text-2xl font-semibold text-white sm:text-3xl">
                {s.name}
              </h1>
              {s.tagline && (
                <p className="mt-1 text-sm text-white/60">{s.tagline}</p>
              )}
              <p className="mt-1 flex items-start gap-1.5 text-sm text-white/40">
                <MapPin className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                <span className="min-w-0">{s.address}</span>
              </p>
            </div>
          </div>

          {!s.open && (
            // The menu still renders. A closed restaurant wants its menu
            // findable — that is most of why this link exists.
            <p className="rounded-lg border border-amber-400/30 bg-amber-400/10 px-3 py-2 text-sm text-amber-300">
              Not taking orders right now.
            </p>
          )}

          <ShareBar name={s.name} />
        </header>

        <div className="py-6">
          <MenuList
            items={s.items.map((i) => ({
              ...i,
              photo_url: i.has_photo ? photoUrl(s, i.item_id) : null,
            }))}
          />
        </div>

        <footer className="border-t border-white/10 py-6 text-center text-xs text-white/25">
          Menu powered by CargoMarket
        </footer>
      </div>
    </main>
  );
}

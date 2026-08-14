"use client";
/**
 * Connect a Shopify or WooCommerce store to this storefront.
 *
 * "Sync from shop" shipped as a button before there was anywhere to say *which*
 * shop. The connectors service has had credential endpoints all along, but no
 * portal called them, so the only way to connect a store was by hand against
 * the API — and pressing Sync without one is not a failure a merchant can act
 * on. The setup belongs next to the action that needs it.
 */
import { useCallback, useEffect, useState } from "react";

import { GlassCard } from "@/components/ui/glass-card";
import {
  connectorsApi,
  SHOP_PLATFORMS,
  type ShopConnection,
} from "@/lib/api/storefront";

const FIELD =
  "w-full rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white " +
  "placeholder:text-white/25 focus:border-cyan-400/50 focus:outline-none";

export function ShopConnections({ vendorId }: { vendorId: string }) {
  const [connections, setConnections] = useState<ShopConnection[] | null>(null);
  const [open, setOpen] = useState(false);
  const [platform, setPlatform] = useState<string>(SHOP_PLATFORMS[0].value);
  const [values, setValues] = useState<Record<string, string>>({});
  const [secret, setSecret] = useState("");
  const [saving, setSaving] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setConnections(await connectorsApi.list());
    } catch {
      // Not worth a red banner over the catalog. The panel shows nothing
      // connected and the merchant can try again.
      setConnections([]);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const spec = SHOP_PLATFORMS.find((p) => p.value === platform) ?? SHOP_PLATFORMS[0];
  const ready =
    secret.trim() !== "" &&
    spec.fields.every((f) => (values[f.key] ?? "").trim() !== "") &&
    !saving;

  async function connect() {
    setSaving(true);
    setErr(null);
    try {
      await connectorsApi.connect({
        platform,
        webhook_secret: secret.trim(),
        config: Object.fromEntries(
          spec.fields.map((f) => [f.key, (values[f.key] ?? "").trim()]),
        ),
        vendorId,
      });
      setValues({});
      setSecret("");
      setOpen(false);
      await reload();
    } catch (e) {
      setErr(e instanceof Error ? e.message : "could not connect that shop");
    } finally {
      setSaving(false);
    }
  }

  async function disconnect(p: string) {
    setErr(null);
    try {
      await connectorsApi.disconnect(p);
      await reload();
    } catch (e) {
      setErr(e instanceof Error ? e.message : "could not disconnect");
    }
  }

  return (
    <GlassCard padding="none">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-white/5 p-4">
        <div className="min-w-0">
          <h2 className="font-heading text-sm font-semibold text-white">Connected shops</h2>
          <p className="mt-0.5 text-xs text-white/45">
            {connections === null
              ? "Checking…"
              : connections.length === 0
                ? "No shop connected — Sync from shop has nothing to pull from yet."
                : `${connections.length} connected. Sync pulls products into this storefront.`}
          </p>
        </div>
        <button
          type="button"
          onClick={() => setOpen((o) => !o)}
          className="rounded-lg border border-white/10 px-3 py-1.5 text-xs text-white/70 hover:bg-white/5"
        >
          {open ? "Cancel" : "Connect a shop"}
        </button>
      </div>

      {connections && connections.length > 0 && (
        <div className="divide-y divide-white/5">
          {connections.map((c) => (
            <div
              key={c.id}
              className="flex flex-col gap-2 p-4 sm:flex-row sm:items-center sm:justify-between"
            >
              <div className="min-w-0">
                <p className="text-sm text-white">
                  {SHOP_PLATFORMS.find((p) => p.value === c.platform)?.label ?? c.platform}
                  {!c.is_active && (
                    <span className="ml-2 text-xs text-amber-300">inactive</span>
                  )}
                </p>
                {/* Generated server-side. The merchant pastes it into their
                    shop so order events reach us. */}
                <p className="mt-1 break-all font-mono text-[11px] text-white/40">
                  {c.webhook_url}
                </p>
              </div>
              <button
                type="button"
                onClick={() => void disconnect(c.platform)}
                className="shrink-0 self-start rounded-lg border border-white/10 px-3 py-1.5 text-xs text-white/50 hover:bg-white/5 hover:text-white/80"
              >
                Disconnect
              </button>
            </div>
          ))}
        </div>
      )}

      {open && (
        <div className="space-y-3 border-t border-white/5 p-4">
          <div>
            <label htmlFor="shop-platform" className="mb-1 block text-xs text-white/60">
              Platform
            </label>
            <select
              id="shop-platform"
              className={FIELD}
              value={platform}
              onChange={(e) => {
                setPlatform(e.target.value);
                setValues({});
              }}
            >
              {SHOP_PLATFORMS.map((p) => (
                <option key={p.value} value={p.value} className="bg-[#0a0f1c]">
                  {p.label}
                </option>
              ))}
            </select>
          </div>

          {spec.fields.map((f) => (
            <div key={f.key}>
              <label htmlFor={`shop-${f.key}`} className="mb-1 block text-xs text-white/60">
                {f.label}
              </label>
              <input
                id={`shop-${f.key}`}
                className={FIELD}
                type={f.secret ? "password" : "text"}
                placeholder={f.placeholder}
                value={values[f.key] ?? ""}
                onChange={(e) =>
                  setValues((v) => ({ ...v, [f.key]: e.target.value }))
                }
              />
            </div>
          ))}

          <div>
            <label htmlFor="shop-secret" className="mb-1 block text-xs text-white/60">
              Webhook signing secret
            </label>
            <input
              id="shop-secret"
              className={FIELD}
              type="password"
              placeholder="the value your shop signs webhooks with"
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
            />
            <p className="mt-1 text-xs text-white/35">
              Copy it from your platform&apos;s webhook settings. Events signed with anything
              else are rejected, so a wrong value here looks like a shop that never sends
              orders rather than an error.
            </p>
          </div>

          {err && (
            <p className="rounded-lg border border-rose-400/20 bg-rose-400/5 px-3 py-2 text-xs text-rose-300">
              {err}
            </p>
          )}

          <button
            type="button"
            onClick={() => void connect()}
            disabled={!ready}
            className="w-full rounded-lg bg-cyan-500/90 px-4 py-2 text-sm font-medium text-[#04121a] hover:bg-cyan-400 disabled:cursor-not-allowed disabled:opacity-40"
          >
            {saving ? "Connecting…" : "Connect"}
          </button>
        </div>
      )}
    </GlassCard>
  );
}

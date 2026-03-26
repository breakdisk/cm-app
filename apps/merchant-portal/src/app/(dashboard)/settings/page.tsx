"use client";
/**
 * Merchant Portal — Settings
 * Merchant profile, pickup addresses, notification preferences, API access.
 */
import { useState } from "react";
import { motion } from "framer-motion";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { variants } from "@/lib/design-system/tokens";

const TABS = ["Profile", "Pickup Addresses", "Notifications", "API Access"] as const;
type Tab = (typeof TABS)[number];

const ADDRESSES = [
  { id: "addr_1", label: "Main Warehouse",  address: "123 Industrial Blvd, Pasig City, Metro Manila 1605", default: true  },
  { id: "addr_2", label: "Cebu Branch",     address: "45 Colon St, Cebu City, Cebu 6000",                 default: false },
  { id: "addr_3", label: "Davao Depot",     address: "Buhangin Rd, Davao City, Davao del Sur 8000",        default: false },
];

const NOTIF_SETTINGS = [
  { event: "Shipment Picked Up",      channels: { email: true,  sms: false, push: true  } },
  { event: "In Transit Update",       channels: { email: false, sms: false, push: true  } },
  { event: "Out for Delivery",        channels: { email: true,  sms: true,  push: true  } },
  { event: "Delivery Successful",     channels: { email: true,  sms: true,  push: true  } },
  { event: "Delivery Failed",         channels: { email: true,  sms: true,  push: true  } },
  { event: "COD Collected",           channels: { email: true,  sms: false, push: false } },
  { event: "Weekly Summary Report",   channels: { email: true,  sms: false, push: false } },
];

export default function MerchantSettingsPage() {
  const [activeTab, setActiveTab] = useState<Tab>("Profile");
  const [notifs, setNotifs] = useState(NOTIF_SETTINGS);

  function toggleNotif(idx: number, ch: keyof typeof NOTIF_SETTINGS[0]["channels"]) {
    setNotifs((prev) =>
      prev.map((n, i) =>
        i === idx ? { ...n, channels: { ...n.channels, [ch]: !n.channels[ch] } } : n
      )
    );
  }

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="p-6 space-y-6"
    >
      <motion.div variants={variants.fadeInUp}>
        <h1 className="text-2xl font-bold text-white font-space-grotesk">Settings</h1>
        <p className="text-white/40 text-sm mt-1">Manage your merchant account and preferences</p>
      </motion.div>

      {/* Tab bar */}
      <motion.div variants={variants.fadeInUp}>
        <div className="flex gap-1 bg-white/[0.03] border border-white/[0.08] rounded-xl p-1 w-fit">
          {TABS.map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`px-4 py-2 rounded-lg text-sm font-medium transition-all ${
                activeTab === tab
                  ? "bg-[#00FF88]/10 text-[#00FF88] border border-[#00FF88]/20"
                  : "text-white/40 hover:text-white/70"
              }`}
            >
              {tab}
            </button>
          ))}
        </div>
      </motion.div>

      {/* Profile */}
      {activeTab === "Profile" && (
        <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <GlassCard title="Business Information">
            <div className="space-y-4">
              {[
                { label: "Business Name",   value: "Acme Trading Corp."          },
                { label: "Merchant ID",     value: "merch-a1b2c3d4", mono: true  },
                { label: "Contact Person",  value: "Juan dela Cruz"               },
                { label: "Email",           value: "ops@acmetrading.ph"           },
                { label: "Phone",           value: "+63 917 123 4567"             },
                { label: "TIN",             value: "123-456-789-000", mono: true  },
              ].map((row) => (
                <div key={row.label} className="flex justify-between items-center py-2 border-b border-white/[0.06]">
                  <span className="text-xs text-white/40 uppercase tracking-widest font-mono">{row.label}</span>
                  <span className={`text-sm text-white font-medium ${row.mono ? "font-mono text-[#00FF88]" : ""}`}>{row.value}</span>
                </div>
              ))}
            </div>
          </GlassCard>

          <GlassCard title="Billing & Plan">
            <div className="space-y-4">
              {[
                { label: "Plan",             value: "Growth",       badge: "green"  },
                { label: "Billing Cycle",    value: "Monthly",      badge: null     },
                { label: "Shipments / mo",   value: "≤ 5,000",      badge: null     },
                { label: "COD Rate",         value: "1.5% + ₱15",   badge: null     },
                { label: "Fragile Surcharge",value: "₱30 / parcel", badge: null     },
                { label: "Status",           value: "Active",       badge: "green"  },
              ].map((row) => (
                <div key={row.label} className="flex justify-between items-center py-2 border-b border-white/[0.06]">
                  <span className="text-xs text-white/40 uppercase tracking-widest font-mono">{row.label}</span>
                  {row.badge ? (
                    <NeonBadge color={row.badge as any}>{row.value}</NeonBadge>
                  ) : (
                    <span className="text-sm text-white font-medium">{row.value}</span>
                  )}
                </div>
              ))}
            </div>
          </GlassCard>
        </motion.div>
      )}

      {/* Pickup Addresses */}
      {activeTab === "Pickup Addresses" && (
        <motion.div variants={variants.fadeInUp} className="space-y-4">
          <div className="flex justify-end">
            <button className="px-4 py-2 text-sm font-medium text-[#050810] bg-[#00FF88] rounded-lg hover:bg-[#00FF88]/90 transition-colors">
              + Add Address
            </button>
          </div>
          {ADDRESSES.map((addr) => (
            <GlassCard key={addr.id}>
              <div className="flex items-start justify-between gap-4">
                <div>
                  <div className="flex items-center gap-3 mb-1">
                    <span className="text-white font-semibold">{addr.label}</span>
                    {addr.default && <NeonBadge color="cyan">Default</NeonBadge>}
                  </div>
                  <p className="text-sm text-white/50">{addr.address}</p>
                </div>
                <div className="flex gap-2 shrink-0">
                  {!addr.default && (
                    <button className="text-xs text-[#00E5FF] hover:text-[#00E5FF]/70">Set Default</button>
                  )}
                  <button className="text-xs text-[#A855F7] hover:text-[#A855F7]/70">Edit</button>
                  {!addr.default && (
                    <button className="text-xs text-[#FF3B5C] hover:text-[#FF3B5C]/70">Remove</button>
                  )}
                </div>
              </div>
            </GlassCard>
          ))}
        </motion.div>
      )}

      {/* Notifications */}
      {activeTab === "Notifications" && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard padding="none">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-white/[0.08]">
                  <th className="text-left px-4 py-3 text-xs text-white/30 uppercase tracking-widest font-mono">Event</th>
                  {(["Email", "SMS", "Push"] as const).map((ch) => (
                    <th key={ch} className="text-center px-4 py-3 text-xs text-white/30 uppercase tracking-widest font-mono">{ch}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {notifs.map((n, i) => (
                  <tr key={n.event} className="border-b border-white/[0.04] hover:bg-white/[0.02]">
                    <td className="px-4 py-3 text-white/70">{n.event}</td>
                    {(["email", "sms", "push"] as const).map((ch) => (
                      <td key={ch} className="px-4 py-3 text-center">
                        <button
                          onClick={() => toggleNotif(i, ch)}
                          className={`w-8 h-4 rounded-full transition-colors relative ${
                            n.channels[ch] ? "bg-[#00E5FF]/40" : "bg-white/10"
                          }`}
                        >
                          <span
                            className={`absolute top-0.5 w-3 h-3 rounded-full transition-all ${
                              n.channels[ch] ? "left-4 bg-[#00E5FF]" : "left-0.5 bg-white/40"
                            }`}
                          />
                        </button>
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </GlassCard>
        </motion.div>
      )}

      {/* API Access */}
      {activeTab === "API Access" && (
        <motion.div variants={variants.fadeInUp} className="space-y-4">
          <GlassCard title="Your API Key">
            <div className="space-y-4">
              <p className="text-sm text-white/40">Use this key to integrate LogisticOS into your order management system. Treat it like a password.</p>
              <div className="flex items-center gap-3 bg-black/30 border border-white/[0.08] rounded-lg p-4">
                <span className="flex-1 font-mono text-[#00FF88] text-sm">lsk_prod_•••••••••••••••••••••••••••••••••</span>
                <button className="text-xs text-white/50 hover:text-white/80 border border-white/[0.08] rounded px-3 py-1.5">Reveal</button>
                <button className="text-xs text-white/50 hover:text-white/80 border border-white/[0.08] rounded px-3 py-1.5">Copy</button>
              </div>
              <div className="flex gap-3">
                <button className="px-4 py-2 text-sm text-[#FFAB00] border border-[#FFAB00]/20 bg-[#FFAB00]/05 rounded-lg hover:bg-[#FFAB00]/10 transition-colors">
                  Rotate Key
                </button>
              </div>
            </div>
          </GlassCard>

          <GlassCard title="Integration Resources">
            <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
              {[
                { title: "API Reference",         sub: "OpenAPI 3.1 spec with all endpoints",   color: "#00E5FF" },
                { title: "Webhook Guide",         sub: "Event types, payload schemas, retries", color: "#A855F7" },
                { title: "Postman Collection",    sub: "Pre-built requests for all endpoints",  color: "#00FF88" },
                { title: "Bulk Import Template",  sub: "CSV template for 500-shipment batches", color: "#FFAB00" },
                { title: "Shopify Plugin",        sub: "Auto-sync orders from Shopify store",   color: "#00E5FF" },
                { title: "WooCommerce Plugin",    sub: "WordPress / WooCommerce integration",   color: "#A855F7" },
              ].map((r) => (
                <div key={r.title} className="p-4 bg-white/[0.03] border border-white/[0.06] rounded-lg hover:border-white/20 transition-colors cursor-pointer">
                  <p className="text-sm font-semibold text-white mb-1">{r.title}</p>
                  <p className="text-xs text-white/40">{r.sub}</p>
                </div>
              ))}
            </div>
          </GlassCard>
        </motion.div>
      )}
    </motion.div>
  );
}

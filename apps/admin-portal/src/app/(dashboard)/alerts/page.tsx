"use client";
/**
 * Admin Portal — Alerts Page
 * Operational alerts: SLA breaches, driver incidents, system anomalies.
 */
import { useState } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import {
  AlertTriangle, AlertCircle, Info, CheckCircle2, X, Bell,
} from "lucide-react";

// ── Types & data ───────────────────────────────────────────────────────────────

type AlertSeverity = "critical" | "warning" | "info";
type AlertCategory = "sla" | "driver" | "system" | "payment" | "fraud";

interface Alert {
  id: string;
  title: string;
  description: string;
  severity: AlertSeverity;
  category: AlertCategory;
  timestamp: string;
  resolved: boolean;
  action?: string;
}

const ALERTS: Alert[] = [
  { id: "A001", title: "SLA Breach — Mindanao Zone",         description: "On-time rate dropped to 82.1% — 13% below contract target. 28 shipments at risk of penalty.",   severity: "critical", category: "sla",     timestamp: "5m ago",    resolved: false, action: "View SLA"     },
  { id: "A002", title: "Driver ETA-992 Unresponsive",         description: "Driver Eduardo Torres (V09) has not sent GPS ping in 47 minutes. Last seen: NLEX Exit 12.",       severity: "critical", category: "driver",  timestamp: "12m ago",   resolved: false, action: "Contact"      },
  { id: "A003", title: "COD Fraud Flag — Batch SH-8820",      description: "Fraud model flagged 14 COD shipments in batch SH-8820 with confidence > 0.90. Hold for review.", severity: "critical", category: "fraud",   timestamp: "18m ago",   resolved: false, action: "Review"       },
  { id: "A004", title: "Kafka Consumer Lag — engagement-svc", description: "Consumer group lag on topic `notifications.send` exceeded 50k messages. Check engagement pod.",  severity: "warning",  category: "system",  timestamp: "24m ago",   resolved: false, action: "View Logs"    },
  { id: "A005", title: "High Failed Delivery Rate — Makati",  description: "Failed delivery rate in Makati jumped to 9.2% (avg: 4.1%) in the last 2 hours.",                 severity: "warning",  category: "sla",     timestamp: "31m ago",   resolved: false, action: "Investigate"  },
  { id: "A006", title: "Payment Gateway Timeout",             description: "PayMongo webhook response time > 10s on 3 consecutive retries. Fallback to Stripe activated.",   severity: "warning",  category: "payment", timestamp: "45m ago",   resolved: false, action: "View Billing" },
  { id: "A007", title: "New Carrier Onboarding Pending",      description: "SpeedEx PH submitted carrier application 2 days ago — requires ops manager review.",              severity: "info",     category: "system",  timestamp: "2h ago",    resolved: false, action: "Review"       },
  { id: "A008", title: "PostgreSQL Replication Lag",          description: "Replica lag hit 4.2 seconds on identity schema — within tolerance but monitor.",                  severity: "info",     category: "system",  timestamp: "3h ago",    resolved: false },
  { id: "A009", title: "SLA Breach — Visayas Zone",           description: "Visayas D+1 rate dropped to 34.2%. This was due to weather event (Category 1 typhoon).",        severity: "critical", category: "sla",     timestamp: "4h ago",    resolved: true  },
  { id: "A010", title: "Driver App Crash Spike",              description: "Driver app version 2.4.1 reported 42 crashes in 1 hour on Android 13. Hotfix deployed.",         severity: "warning",  category: "system",  timestamp: "6h ago",    resolved: true  },
];

const SEVERITY_CONFIG: Record<AlertSeverity, { icon: React.ReactNode; variant: "red" | "amber" | "cyan"; label: string; borderColor: string }> = {
  critical: { icon: <AlertCircle size={14} className="text-red-signal"    />, variant: "red",   label: "Critical", borderColor: "border-red-signal/20"   },
  warning:  { icon: <AlertTriangle size={14} className="text-amber-signal" />, variant: "amber", label: "Warning",  borderColor: "border-amber-signal/20" },
  info:     { icon: <Info size={14} className="text-cyan-neon"             />, variant: "cyan",  label: "Info",     borderColor: "border-cyan-neon/20"    },
};

const CATEGORY_LABEL: Record<AlertCategory, string> = {
  sla:     "SLA",
  driver:  "Driver",
  system:  "System",
  payment: "Payment",
  fraud:   "Fraud",
};

const SUMMARY = {
  critical: ALERTS.filter(a => a.severity === "critical" && !a.resolved).length,
  warning:  ALERTS.filter(a => a.severity === "warning"  && !a.resolved).length,
  info:     ALERTS.filter(a => a.severity === "info"     && !a.resolved).length,
};

export default function AlertsPage() {
  const [resolved, setResolved] = useState<Set<string>>(
    new Set(ALERTS.filter(a => a.resolved).map(a => a.id))
  );
  const [showResolved, setShowResolved] = useState(false);
  const [severityFilter, setSeverityFilter] = useState<AlertSeverity | "all">("all");

  function resolve(id: string) {
    setResolved(prev => new Set([...prev, id]));
  }

  const visible = ALERTS.filter(a => {
    const isResolved = resolved.has(a.id);
    if (!showResolved && isResolved) return false;
    if (severityFilter !== "all" && a.severity !== severityFilter) return false;
    return true;
  });

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-5 p-6"
    >
      {/* Header */}
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
        <div>
          <h1 className="font-heading text-2xl font-bold text-white flex items-center gap-2">
            <Bell size={22} className="text-red-signal" />
            Alerts
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            {SUMMARY.critical} critical · {SUMMARY.warning} warnings · {SUMMARY.info} info
          </p>
        </div>
        <button
          onClick={() => setShowResolved(v => !v)}
          className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
        >
          <CheckCircle2 size={12} />
          {showResolved ? "Hide Resolved" : "Show Resolved"}
        </button>
      </motion.div>

      {/* Summary row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-3 gap-3">
        {[
          { label: "Critical",  count: SUMMARY.critical, color: "text-red-signal",   bg: "bg-red-signal/10 border-red-signal/20"   },
          { label: "Warnings",  count: SUMMARY.warning,  color: "text-amber-signal", bg: "bg-amber-signal/10 border-amber-signal/20" },
          { label: "Info",      count: SUMMARY.info,     color: "text-cyan-neon",    bg: "bg-cyan-surface border-cyan-neon/20"       },
        ].map((s) => (
          <div key={s.label} className={`rounded-xl border px-4 py-3 ${s.bg}`}>
            <p className={`font-heading text-3xl font-bold ${s.color}`}>{s.count}</p>
            <p className="text-xs text-white/40 font-mono mt-0.5">{s.label} active</p>
          </div>
        ))}
      </motion.div>

      {/* Filter */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard>
          <div className="flex items-center gap-1.5">
            {(["all", "critical", "warning", "info"] as const).map((s) => (
              <button
                key={s}
                onClick={() => setSeverityFilter(s)}
                className={`rounded-full px-3 py-1 text-xs font-medium capitalize transition-all ${
                  severityFilter === s
                    ? "bg-canvas border border-glass-border-bright text-white"
                    : "text-white/40 border border-glass-border hover:text-white"
                }`}
              >
                {s}
              </button>
            ))}
          </div>
        </GlassCard>
      </motion.div>

      {/* Alert list */}
      <motion.div variants={variants.fadeInUp} className="flex flex-col gap-2">
        {visible.map((alert) => {
          const { icon, variant, label, borderColor } = SEVERITY_CONFIG[alert.severity];
          const isResolved = resolved.has(alert.id);
          return (
            <div
              key={alert.id}
              className={`rounded-xl border bg-glass-100 px-4 py-4 transition-all ${
                isResolved ? "opacity-50 border-glass-border" : borderColor
              }`}
            >
              <div className="flex items-start gap-3">
                <div className="mt-0.5 flex-shrink-0">{icon}</div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 mb-1">
                    <p className={`text-sm font-semibold ${isResolved ? "text-white/40 line-through" : "text-white"}`}>
                      {alert.title}
                    </p>
                    <NeonBadge variant={variant}>{label}</NeonBadge>
                    <NeonBadge variant="cyan">{CATEGORY_LABEL[alert.category]}</NeonBadge>
                    {isResolved && <NeonBadge variant="green"><CheckCircle2 size={10} className="mr-1 inline" />Resolved</NeonBadge>}
                  </div>
                  <p className="text-xs text-white/50 mb-2">{alert.description}</p>
                  <div className="flex items-center gap-3">
                    <span className="text-2xs font-mono text-white/30">{alert.timestamp}</span>
                    {alert.action && !isResolved && (
                      <button className="text-2xs font-mono text-cyan-neon hover:underline">{alert.action}</button>
                    )}
                    {!isResolved && (
                      <button
                        onClick={() => resolve(alert.id)}
                        className="ml-auto text-2xs font-mono text-white/30 hover:text-green-signal transition-colors flex items-center gap-1"
                      >
                        <CheckCircle2 size={11} /> Mark Resolved
                      </button>
                    )}
                  </div>
                </div>
                {!isResolved && (
                  <button onClick={() => resolve(alert.id)} className="flex-shrink-0 rounded p-1 text-white/20 hover:text-white/60 transition-colors">
                    <X size={14} />
                  </button>
                )}
              </div>
            </div>
          );
        })}

        {visible.length === 0 && (
          <GlassCard className="text-center py-10">
            <CheckCircle2 size={28} className="text-green-signal mx-auto mb-2" />
            <p className="text-sm font-semibold text-white">All clear</p>
            <p className="text-xs text-white/40 font-mono mt-1">No active alerts matching your filter</p>
          </GlassCard>
        )}
      </motion.div>
    </motion.div>
  );
}

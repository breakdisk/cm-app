"use client";
/**
 * Admin Portal — AI Agents Page
 * Live status of all LogisticOS AI agents: dispatch, support, marketing, fraud.
 */
import { useState } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import {
  AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
} from "recharts";
import { Bot, Zap, Brain, ShieldCheck, Megaphone, Headphones, Route, RefreshCw } from "lucide-react";

// ── Types & data ───────────────────────────────────────────────────────────────

type AgentStatus = "running" | "idle" | "error" | "training";

interface AIAgent {
  id: string;
  name: string;
  description: string;
  status: AgentStatus;
  model: string;
  invocations_today: number;
  avg_latency_ms: number;
  success_rate: number;
  last_action: string;
  last_action_time: string;
  icon: React.ReactNode;
}

const AGENTS: AIAgent[] = [
  {
    id: "dispatch",
    name: "Dispatch Agent",
    description: "Smart driver assignment, VRP optimization, re-routing on delays",
    status: "running",
    model: "claude-opus-4-6 + ONNX VRP",
    invocations_today: 2840,
    avg_latency_ms: 320,
    success_rate: 99.1,
    last_action: "Assigned driver JDC to route R-2847 (18 stops, Makati)",
    last_action_time: "12s ago",
    icon: <Route size={18} className="text-cyan-neon" />,
  },
  {
    id: "support",
    name: "Support Agent",
    description: "WhatsApp & chat support, reschedule requests, complaint triage",
    status: "running",
    model: "claude-sonnet-4-6",
    invocations_today: 1240,
    avg_latency_ms: 890,
    success_rate: 96.4,
    last_action: "Rescheduled delivery for LS-A1B2C3 to March 18 (customer request)",
    last_action_time: "2m ago",
    icon: <Headphones size={18} className="text-purple-plasma" />,
  },
  {
    id: "marketing",
    name: "Marketing Agent",
    description: "Campaign copy generation, send-time optimization, A/B test selection",
    status: "running",
    model: "claude-sonnet-4-6",
    invocations_today: 84,
    avg_latency_ms: 1240,
    success_rate: 100,
    last_action: "Generated 3 WhatsApp variants for 'Post-Delivery Upsell' campaign",
    last_action_time: "15m ago",
    icon: <Megaphone size={18} className="text-amber-signal" />,
  },
  {
    id: "fraud",
    name: "Fraud Detection Agent",
    description: "Payment fraud scoring, COD refusal patterns, shipment anomaly detection",
    status: "running",
    model: "ONNX GradientBoost + claude-haiku-4-5",
    invocations_today: 8420,
    avg_latency_ms: 45,
    success_rate: 99.8,
    last_action: "Flagged shipment SH-992 for COD fraud review (score: 0.94)",
    last_action_time: "34s ago",
    icon: <ShieldCheck size={18} className="text-red-signal" />,
  },
  {
    id: "logistics-planner",
    name: "Logistics Planner",
    description: "Daily route planning, hub capacity optimization, demand forecasting",
    status: "idle",
    model: "claude-opus-4-6 + ONNX Forecast",
    invocations_today: 12,
    avg_latency_ms: 2800,
    success_rate: 100,
    last_action: "Generated tomorrow's route plan: 47 drivers, 1,320 stops across Metro Manila",
    last_action_time: "2h ago",
    icon: <Brain size={18} className="text-green-signal" />,
  },
  {
    id: "customer-intelligence",
    name: "Customer Intelligence",
    description: "CLV scoring, churn prediction, delivery preference modeling",
    status: "training",
    model: "ONNX XGBoost (retraining)",
    invocations_today: 0,
    avg_latency_ms: 0,
    success_rate: 0,
    last_action: "Model retraining on 90-day behavioral dataset (ETA: 23min)",
    last_action_time: "now",
    icon: <Zap size={18} className="text-purple-plasma" />,
  },
];

const STATUS_CONFIG: Record<AgentStatus, { label: string; variant: "green" | "cyan" | "amber" | "red" | "purple" }> = {
  running:  { label: "Running",  variant: "green"  },
  idle:     { label: "Idle",     variant: "cyan"   },
  error:    { label: "Error",    variant: "red"    },
  training: { label: "Training", variant: "purple" },
};

const KPI = [
  { label: "Active Agents",      value: 4,     trend: 0,    color: "green"  as const, format: "number"  as const },
  { label: "Invocations Today",  value: 12600, trend: +18.4, color: "cyan"   as const, format: "number"  as const },
  { label: "Avg Latency P99",    value: 890,   trend: -12.0, color: "purple" as const, format: "number"  as const },
  { label: "Avg Success Rate",   value: 98.8,  trend: +0.4,  color: "amber"  as const, format: "percent" as const },
];

const INVOCATION_TREND = Array.from({ length: 24 }, (_, i) => ({
  hour: `${i}:00`,
  dispatch: Math.floor(80 + Math.sin(i / 4) * 40 + Math.random() * 20),
  support:  Math.floor(30 + Math.sin(i / 3) * 20 + Math.random() * 10),
  fraud:    Math.floor(200 + Math.sin(i / 5) * 80 + Math.random() * 30),
}));

export default function AIAgentsPage() {
  const [selectedAgent, setSelectedAgent] = useState<string | null>(null);

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
            <Bot size={22} className="text-purple-plasma" />
            AI Agents
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">Agentic runtime · MCP tool dispatch · claude-opus-4-6</p>
        </div>
        <button className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors">
          <RefreshCw size={12} /> Refresh
        </button>
      </motion.div>

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {KPI.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* Invocation trend */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard glow="purple">
          <div className="flex items-center justify-between mb-4">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">Agent Invocations — Today (24h)</h2>
              <p className="text-2xs font-mono text-white/30">Dispatch · Support · Fraud</p>
            </div>
          </div>
          <ResponsiveContainer width="100%" height={160}>
            <AreaChart data={INVOCATION_TREND} margin={{ top: 0, right: 0, bottom: 0, left: -24 }}>
              <defs>
                <linearGradient id="grad-disp" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#00E5FF" stopOpacity={0.25} />
                  <stop offset="95%" stopColor="#00E5FF" stopOpacity={0}    />
                </linearGradient>
                <linearGradient id="grad-sup" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#A855F7" stopOpacity={0.25} />
                  <stop offset="95%" stopColor="#A855F7" stopOpacity={0}    />
                </linearGradient>
                <linearGradient id="grad-fraud" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#FF3B5C" stopOpacity={0.2}  />
                  <stop offset="95%" stopColor="#FF3B5C" stopOpacity={0}    />
                </linearGradient>
              </defs>
              <CartesianGrid stroke="rgba(255,255,255,0.04)" strokeDasharray="4 4" vertical={false} />
              <XAxis dataKey="hour" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 9, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} interval={3} />
              <YAxis tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <Tooltip
                contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }}
                labelStyle={{ color: "rgba(255,255,255,0.4)" }}
              />
              <Area type="monotone" dataKey="fraud"    stroke="#FF3B5C" fill="url(#grad-fraud)" strokeWidth={1.5} />
              <Area type="monotone" dataKey="dispatch" stroke="#00E5FF" fill="url(#grad-disp)"  strokeWidth={1.5} />
              <Area type="monotone" dataKey="support"  stroke="#A855F7" fill="url(#grad-sup)"   strokeWidth={1.5} />
            </AreaChart>
          </ResponsiveContainer>
        </GlassCard>
      </motion.div>

      {/* Agent cards */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 gap-3 lg:grid-cols-2">
        {AGENTS.map((agent) => {
          const { label, variant } = STATUS_CONFIG[agent.status];
          const isSelected = selectedAgent === agent.id;
          return (
            <GlassCard
              key={agent.id}
              className={`cursor-pointer transition-all ${isSelected ? "border-purple-plasma/40" : "hover:border-glass-border-bright"}`}
              glow={isSelected ? "purple" : undefined}
              onClick={() => setSelectedAgent(isSelected ? null : agent.id)}
            >
              <div className="flex items-start gap-3 mb-3">
                <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-xl bg-glass-200 border border-glass-border">
                  {agent.icon}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center justify-between gap-2">
                    <p className="text-sm font-semibold text-white truncate">{agent.name}</p>
                    <NeonBadge variant={variant} dot={agent.status === "running"}>{label}</NeonBadge>
                  </div>
                  <p className="text-2xs font-mono text-white/40 mt-0.5 truncate">{agent.model}</p>
                </div>
              </div>

              <p className="text-xs text-white/50 mb-3">{agent.description}</p>

              {/* Stats row */}
              <div className="grid grid-cols-3 gap-2 mb-3">
                {[
                  { label: "Invocations", value: agent.invocations_today > 0 ? agent.invocations_today.toLocaleString() : "—" },
                  { label: "Avg Latency", value: agent.avg_latency_ms > 0 ? `${agent.avg_latency_ms}ms` : "—" },
                  { label: "Success",     value: agent.success_rate > 0 ? `${agent.success_rate}%` : "—" },
                ].map((s) => (
                  <div key={s.label} className="rounded-lg bg-glass-100 px-2.5 py-2">
                    <p className="text-2xs font-mono text-white/30 uppercase">{s.label}</p>
                    <p className="text-sm font-bold text-white mt-0.5">{s.value}</p>
                  </div>
                ))}
              </div>

              {/* Last action */}
              <div className="rounded-lg bg-glass-100 border border-glass-border px-3 py-2">
                <p className="text-2xs font-mono text-white/30 mb-0.5">Last action · {agent.last_action_time}</p>
                <p className="text-xs text-white/60 line-clamp-2">{agent.last_action}</p>
              </div>
            </GlassCard>
          );
        })}
      </motion.div>
    </motion.div>
  );
}

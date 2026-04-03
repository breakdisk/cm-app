"use client";

import { Brain, Sparkles, Bot, Network, Cpu, Wand2 } from "lucide-react";

const agents = [
  {
    icon: Brain,
    name: "Dispatch Agent",
    desc: "Optimizes routes, assigns drivers, predicts delays before they happen.",
    status: "Active",
    metrics: "12ms avg response",
  },
  {
    icon: Bot,
    name: "Support Agent",
    desc: "Handles customer WhatsApp/chat inquiries. Books reschedules, checks ETAs.",
    status: "Active",
    metrics: "94% resolution rate",
  },
  {
    icon: Sparkles,
    name: "Marketing Agent",
    desc: "Generates next-shipment campaigns, personalizes messaging by CLV segment.",
    status: "Active",
    metrics: "+34% retention lift",
  },
  {
    icon: Network,
    name: "Carrier Agent",
    desc: "Auto-selects the best carrier per zone based on SLA, cost, and performance.",
    status: "Active",
    metrics: "23% cost reduction",
  },
  {
    icon: Cpu,
    name: "Fraud Agent",
    desc: "Scores every COD order for fraud risk. Flags anomalies before dispatch.",
    status: "Active",
    metrics: "0.02% false positive",
  },
  {
    icon: Wand2,
    name: "Ops Copilot",
    desc: "Natural language interface to your entire operation. Ask anything.",
    status: "Beta",
    metrics: "GPT-4 + Claude",
  },
];

export default function AISection() {
  return (
    <section id="ai" className="py-24 lg:py-32 relative overflow-hidden">
      {/* Background */}
      <div className="absolute inset-0 aurora-bg opacity-20 pointer-events-none" />
      <div className="absolute inset-0 bg-[#050810]/70 pointer-events-none" />
      <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[800px] h-[400px] bg-purple-plasma/8 rounded-full blur-[120px] pointer-events-none" />

      <div className="max-w-7xl mx-auto px-6 lg:px-8 relative z-10">
        <div className="text-center mb-16">
          <div className="inline-flex items-center gap-2 px-4 py-1.5 rounded-full glass-panel border border-purple-plasma/30 mb-6 text-xs font-medium text-purple-plasma">
            <Brain className="w-3 h-3" />
            Powered by Anthropic Claude + OpenAI
          </div>
          <h2
            className="text-4xl lg:text-5xl font-bold text-white mb-4"
            style={{ fontFamily: "'Space Grotesk', sans-serif" }}
          >
            AI Agents That Run{" "}
            <span className="text-gradient-brand">Your Logistics</span>
          </h2>
          <p className="text-slate-400 text-lg max-w-2xl mx-auto">
            Not just analytics. Autonomous AI agents that take actions — dispatch
            drivers, engage customers, detect fraud — 24/7, without human input.
          </p>
        </div>

        {/* MCP architecture callout */}
        <div className="glass-panel-bright rounded-2xl border border-purple-plasma/20 p-5 mb-10 max-w-3xl mx-auto">
          <div className="flex items-start gap-4">
            <div className="w-10 h-10 rounded-xl bg-purple-plasma/10 border border-purple-plasma/20 flex items-center justify-center flex-shrink-0">
              <Network className="w-5 h-5 text-purple-plasma" />
            </div>
            <div>
              <div className="text-sm font-semibold text-white mb-1">
                Model Context Protocol (MCP) Architecture
              </div>
              <p className="text-xs text-slate-500 leading-relaxed">
                Every service exposes an MCP server. AI agents call <code className="font-mono text-cyan-neon text-xs">assign_driver</code>,{" "}
                <code className="font-mono text-cyan-neon text-xs">send_notification</code>,{" "}
                <code className="font-mono text-cyan-neon text-xs">get_churn_score</code> — standardized tools, fully audited and reversible.
                Enterprise tenants can plug in their own external AI workflows via the same protocol.
              </p>
            </div>
          </div>
        </div>

        {/* Agent cards */}
        <div className="grid sm:grid-cols-2 lg:grid-cols-3 gap-4">
          {agents.map((agent) => (
            <div
              key={agent.name}
              className="group glass-panel rounded-2xl border border-purple-plasma/15 p-5 hover:border-purple-plasma/35 hover:bg-purple-plasma/[0.04] transition-all duration-300"
            >
              <div className="flex items-start justify-between mb-4">
                <div className="w-10 h-10 rounded-xl bg-purple-plasma/10 border border-purple-plasma/20 flex items-center justify-center group-hover:scale-110 transition-transform">
                  <agent.icon className="w-5 h-5 text-purple-plasma" />
                </div>
                <span
                  className={`px-2.5 py-1 rounded-full text-xs font-medium border ${
                    agent.status === "Active"
                      ? "bg-green-signal/10 text-green-signal border-green-signal/20"
                      : "bg-amber-signal/10 text-amber-signal border-amber-signal/20"
                  }`}
                >
                  {agent.status}
                </span>
              </div>

              <h3
                className="text-sm font-semibold text-white mb-1.5"
                style={{ fontFamily: "'Space Grotesk', sans-serif" }}
              >
                {agent.name}
              </h3>
              <p className="text-xs text-slate-500 leading-relaxed mb-4">{agent.desc}</p>

              <div className="flex items-center gap-1.5 pt-3 border-t border-white/[0.05]">
                <span className="w-1.5 h-1.5 rounded-full bg-cyan-neon animate-pulse-neon" />
                <span
                  className="text-xs text-cyan-neon"
                  style={{ fontFamily: "'JetBrains Mono', monospace" }}
                >
                  {agent.metrics}
                </span>
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

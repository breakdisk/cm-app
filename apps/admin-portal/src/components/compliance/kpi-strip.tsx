import type { ComplianceProfile } from "@/lib/api/compliance";

interface Props {
  profiles: ComplianceProfile[];
}

const KPI_CONFIG = [
  { label: "Compliant",      status: "compliant",          color: "text-green-signal",  border: "border-green-glow/20",  bg: "bg-green-surface/10"  },
  { label: "Pending Review", status: "under_review",       color: "text-cyan-neon",     border: "border-cyan-glow/20",   bg: "bg-cyan-surface/10"   },
  { label: "Expiring Soon",  status: "expiring_soon",      color: "text-amber-signal",  border: "border-amber-glow/20",  bg: "bg-amber-surface/10"  },
  { label: "Suspended",      status: "suspended",          color: "text-red-signal",    border: "border-red-glow/20",    bg: "bg-red-surface/10"    },
] as const;

export function ComplianceKpiStrip({ profiles }: Props) {
  const count = (status: string) =>
    profiles.filter((p) => p.overall_status === status).length;

  return (
    <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
      {KPI_CONFIG.map((kpi) => (
        <div
          key={kpi.label}
          className={`rounded-xl p-4 border ${kpi.bg} ${kpi.border}`}
        >
          <div className={`text-3xl font-mono font-bold ${kpi.color}`}>
            {count(kpi.status)}
          </div>
          <div className="text-xs uppercase tracking-widest text-white/40 mt-1">
            {kpi.label}
          </div>
        </div>
      ))}
    </div>
  );
}

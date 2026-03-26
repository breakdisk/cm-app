import { cn } from "@/lib/design-system/cn";
import { ShieldCheck, ShieldAlert, ShieldX, Shield } from "lucide-react";

type ComplianceStatus =
  | "compliant" | "expiring_soon" | "expired"
  | "suspended" | "under_review" | "pending_submission" | "rejected";

interface Props {
  status:        ComplianceStatus;
  expiryDetail?: string;   // e.g. "License · 12d"
}

const CONFIG: Record<ComplianceStatus, {
  label:  string;
  icon:   React.ReactNode;
  color:  string;
  bg:     string;
  border: string;
  pulse:  boolean;
}> = {
  compliant:          { label: "Compliant",     icon: <ShieldCheck className="h-3 w-3" />, color: "text-green-signal",  bg: "bg-green-surface",  border: "border-green-glow/20",  pulse: false },
  expiring_soon:      { label: "Expiring Soon", icon: <ShieldAlert  className="h-3 w-3" />, color: "text-amber-signal", bg: "bg-amber-surface",  border: "border-amber-glow/25",  pulse: true  },
  expired:            { label: "Expired",       icon: <ShieldX      className="h-3 w-3" />, color: "text-amber-signal", bg: "bg-amber-surface",  border: "border-amber-glow/25",  pulse: false },
  suspended:          { label: "Suspended",     icon: <ShieldX      className="h-3 w-3" />, color: "text-red-signal",   bg: "bg-red-surface",    border: "border-red-glow/25",    pulse: false },
  under_review:       { label: "Under Review",  icon: <Shield       className="h-3 w-3" />, color: "text-cyan-neon",    bg: "bg-cyan-surface",   border: "border-cyan-glow/20",   pulse: false },
  pending_submission: { label: "Docs Pending",  icon: <Shield       className="h-3 w-3" />, color: "text-white/40",     bg: "bg-glass-100",      border: "border-glass-border",   pulse: false },
  rejected:           { label: "Docs Rejected", icon: <ShieldX      className="h-3 w-3" />, color: "text-red-signal",   bg: "bg-red-surface",    border: "border-red-glow/25",    pulse: false },
};

export function ComplianceBadge({ status, expiryDetail }: Props) {
  const cfg = CONFIG[status] ?? CONFIG.pending_submission;
  return (
    <div className={cn("flex items-center gap-1.5 rounded-lg px-2.5 py-1.5 border", cfg.bg, cfg.border)}>
      <div className={cn(cfg.color, cfg.pulse && "animate-pulse")}>{cfg.icon}</div>
      <div>
        <div className={cn("text-2xs font-semibold font-mono", cfg.color)}>{cfg.label}</div>
        {expiryDetail && (
          <div className="text-2xs text-white/25 font-mono">{expiryDetail}</div>
        )}
      </div>
    </div>
  );
}

export function canAssign(status: ComplianceStatus): boolean {
  return ["compliant", "expiring_soon", "expired"].includes(status);
}

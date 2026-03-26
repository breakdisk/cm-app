/**
 * NeonBadge — Customer Portal
 *
 * Pill badge with neon color variants matching the LogisticOS design system.
 * Used for shipment status labels, delivery state, notifications.
 *
 * Usage:
 *   <NeonBadge variant="green" dot pulse>Delivered</NeonBadge>
 *   <NeonBadge variant="amber">Pending</NeonBadge>
 *
 * Exports both named (NeonBadge) and default.
 */

import type { ReactNode, HTMLAttributes } from "react";

// ─── Types ─────────────────────────────────────────────────────────────────────

export type BadgeVariant =
  | "cyan"
  | "purple"
  | "green"
  | "amber"
  | "red"
  | "muted";

export interface NeonBadgeProps extends HTMLAttributes<HTMLSpanElement> {
  variant?: BadgeVariant;
  /** Show animated pulsing dot indicator */
  pulse?: boolean;
  /** Show a static dot indicator (no animation) */
  dot?: boolean;
  children: ReactNode;
}

// ─── Token maps (kept in-component — no shared lib dep for customer portal) ────

const VARIANT_STYLES: Record<
  BadgeVariant,
  { text: string; bg: string; border: string; shadow?: string }
> = {
  cyan: {
    text:   "#00E5FF",
    bg:     "rgba(0,229,255,0.06)",
    border: "rgba(0,229,255,0.30)",
    shadow: "0 0 8px rgba(0,229,255,0.5)",
  },
  purple: {
    text:   "#A855F7",
    bg:     "rgba(168,85,247,0.06)",
    border: "rgba(168,85,247,0.30)",
  },
  green: {
    text:   "#00FF88",
    bg:     "rgba(0,255,136,0.06)",
    border: "rgba(0,255,136,0.30)",
    shadow: "0 0 8px rgba(0,255,136,0.5)",
  },
  amber: {
    text:   "#FFAB00",
    bg:     "rgba(255,171,0,0.06)",
    border: "rgba(255,171,0,0.30)",
  },
  red: {
    text:   "#FF3B5C",
    bg:     "rgba(255,59,92,0.06)",
    border: "rgba(255,59,92,0.30)",
  },
  muted: {
    text:   "rgba(255,255,255,0.50)",
    bg:     "rgba(255,255,255,0.06)",
    border: "rgba(255,255,255,0.08)",
  },
};

const DOT_COLOR: Record<BadgeVariant, string> = {
  cyan:   "#00E5FF",
  purple: "#A855F7",
  green:  "#00FF88",
  amber:  "#FFAB00",
  red:    "#FF3B5C",
  muted:  "rgba(255,255,255,0.30)",
};

// ─── Component ─────────────────────────────────────────────────────────────────

export function NeonBadge({
  variant = "cyan",
  pulse = false,
  dot = false,
  children,
  className,
  style,
  ...rest
}: NeonBadgeProps) {
  const s = VARIANT_STYLES[variant];

  return (
    <span
      {...rest}
      className={[
        "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5",
        // Font — mirrors the shared design system
        "text-[0.65rem] leading-[1rem] font-medium uppercase tracking-wider",
        // Monospace for numeric badges (AWB codes, counts, rates)
        "font-mono",
        className ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
      style={{
        color: s.text,
        background: s.bg,
        borderColor: s.border,
        boxShadow: s.shadow,
        ...style,
      }}
    >
      {/* Dot indicator */}
      {(dot || pulse) && (
        <span
          className="relative flex flex-shrink-0"
          style={{ height: "6px", width: "6px" }}
          aria-hidden
        >
          {/* Outer ping ring */}
          {pulse && (
            <span
              className="absolute inline-flex h-full w-full rounded-full"
              style={{
                background: DOT_COLOR[variant],
                opacity: 0.75,
                animation: "badge-beacon 1.5s ease-out infinite",
              }}
            />
          )}
          {/* Solid core dot */}
          <span
            className="relative inline-flex rounded-full"
            style={{
              height: "6px",
              width: "6px",
              background: DOT_COLOR[variant],
            }}
          />
        </span>
      )}
      {children}

      {/* Beacon keyframe injected once per page via a style tag */}
      {pulse && (
        <style>{`
          @keyframes badge-beacon {
            0%   { transform: scale(1);   opacity: 0.8; }
            100% { transform: scale(2.5); opacity: 0;   }
          }
        `}</style>
      )}
    </span>
  );
}

export default NeonBadge;

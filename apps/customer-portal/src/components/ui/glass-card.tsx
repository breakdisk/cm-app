/**
 * GlassCard — Customer Portal
 *
 * Glassmorphism card component: translucent panel with backdrop-blur,
 * a subtle white border, and an optional neon glow on hover.
 *
 * Usage:
 *   <GlassCard glow="cyan" accent size="md">…</GlassCard>
 *
 * Exports both named (GlassCard) and default.
 */
"use client";

import { type HTMLAttributes, forwardRef } from "react";

// ─── Types ─────────────────────────────────────────────────────────────────────

export type GlowColor = "cyan" | "purple" | "green" | "amber" | "red" | "none";
export type CardSize  = "sm" | "md" | "lg";
export type PaddingOverride = "none" | "sm" | "md" | "lg";

export interface GlassCardProps extends HTMLAttributes<HTMLDivElement> {
  /** Neon glow accent color applied on hover */
  glow?: GlowColor;
  /** Card size controls the glass depth and default padding */
  size?: CardSize;
  /** Render a 1px top-edge accent line in the glow color */
  accent?: boolean;
  /** Override default padding */
  padding?: PaddingOverride;
}

// ─── Maps ──────────────────────────────────────────────────────────────────────

const GLOW_HOVER: Record<GlowColor, string> = {
  cyan:   "0 0 20px rgba(0,229,255,0.35), 0 0 40px rgba(0,229,255,0.15)",
  purple: "0 0 20px rgba(168,85,247,0.35), 0 0 40px rgba(168,85,247,0.15)",
  green:  "0 0 20px rgba(0,255,136,0.35), 0 0 40px rgba(0,255,136,0.15)",
  amber:  "0 0 20px rgba(255,171,0,0.35),  0 0 40px rgba(255,171,0,0.15)",
  red:    "0 0 20px rgba(255,59,92,0.35),  0 0 40px rgba(255,59,92,0.15)",
  none:   "",
};

const ACCENT_COLOR: Record<GlowColor, string> = {
  cyan:   "#00E5FF",
  purple: "#A855F7",
  green:  "#00FF88",
  amber:  "#FFAB00",
  red:    "#FF3B5C",
  none:   "transparent",
};

const SIZE_SHADOW: Record<CardSize, string> = {
  sm: "0 4px 16px rgba(0,0,0,0.35), inset 0 1px 0 rgba(255,255,255,0.05)",
  md: "0 4px 24px rgba(0,0,0,0.40), inset 0 1px 0 rgba(255,255,255,0.06)",
  lg: "0 8px 40px rgba(0,0,0,0.50), inset 0 1px 0 rgba(255,255,255,0.08)",
};

const SIZE_BG: Record<CardSize, string> = {
  sm: "rgba(13, 20, 34, 0.55)",
  md: "rgba(13, 20, 34, 0.65)",
  lg: "rgba(13, 20, 34, 0.75)",
};

const PADDING_CLASS: Record<PaddingOverride, string> = {
  none: "p-0",
  sm:   "p-4",
  md:   "p-6",
  lg:   "p-8",
};

const DEFAULT_PADDING: Record<CardSize, string> = {
  sm: "p-4",
  md: "p-6",
  lg: "p-8",
};

// ─── Component ─────────────────────────────────────────────────────────────────

export const GlassCard = forwardRef<HTMLDivElement, GlassCardProps>(
  function GlassCard(
    {
      glow = "none",
      size = "md",
      accent = false,
      padding,
      children,
      className,
      style,
      onMouseEnter,
      onMouseLeave,
      ...rest
    },
    ref
  ) {
    const paddingClass =
      padding !== undefined ? PADDING_CLASS[padding] : DEFAULT_PADDING[size];
    const baseShadow   = SIZE_SHADOW[size];
    const hoverShadow  = glow !== "none" ? GLOW_HOVER[glow] : "";

    // We manage hover state manually so we can apply shadow via inline style
    // (Tailwind can't safely extend box-shadow with arbitrary neon values at runtime)
    function handleMouseEnter(e: React.MouseEvent<HTMLDivElement>) {
      if (hoverShadow) {
        (e.currentTarget as HTMLDivElement).style.boxShadow =
          `${hoverShadow}, ${baseShadow}`;
        if (glow !== "none") {
          (e.currentTarget as HTMLDivElement).style.borderColor =
            `${ACCENT_COLOR[glow]}40`;
        }
      }
      onMouseEnter?.(e);
    }

    function handleMouseLeave(e: React.MouseEvent<HTMLDivElement>) {
      (e.currentTarget as HTMLDivElement).style.boxShadow = baseShadow;
      (e.currentTarget as HTMLDivElement).style.borderColor =
        "rgba(255,255,255,0.08)";
      onMouseLeave?.(e);
    }

    return (
      <div
        ref={ref}
        {...rest}
        onMouseEnter={handleMouseEnter}
        onMouseLeave={handleMouseLeave}
        className={[
          "relative overflow-hidden rounded-2xl border transition-all duration-300",
          paddingClass,
          className ?? "",
        ]
          .filter(Boolean)
          .join(" ")}
        style={{
          background: SIZE_BG[size],
          backdropFilter: "blur(16px)",
          WebkitBackdropFilter: "blur(16px)",
          borderColor: "rgba(255,255,255,0.08)",
          boxShadow: baseShadow,
          // Top accent line via linear-gradient overlay
          ...(accent && glow !== "none"
            ? {
                backgroundImage: `linear-gradient(${ACCENT_COLOR[glow]}, ${ACCENT_COLOR[glow]}) top / 100% 1px no-repeat, none`,
              }
            : {}),
          ...style,
        }}
      >
        {/* Top-edge accent line (separate element for precise opacity control) */}
        {accent && glow !== "none" && (
          <div
            aria-hidden
            className="pointer-events-none absolute inset-x-0 top-0 h-px"
            style={{
              background: ACCENT_COLOR[glow],
              opacity: 0.65,
            }}
          />
        )}
        {children}
      </div>
    );
  }
);

export default GlassCard;

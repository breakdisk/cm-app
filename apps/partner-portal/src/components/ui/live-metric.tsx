"use client";
import { useEffect, useRef } from "react";
import { motion, useMotionValue, useSpring, useTransform } from "framer-motion";
import { cn } from "@/lib/design-system/cn";
import type { BadgeVariant } from "./neon-badge";

interface LiveMetricProps {
  label: string;
  value: number;
  unit?: string;
  trend?: number;       // positive = up, negative = down
  color?: "cyan" | "purple" | "green" | "amber" | "red";
  format?: "number" | "percent" | "currency" | "duration";
  live?: boolean;       // shows pulsing live indicator
  className?: string;
}

const colorMap = {
  cyan:   "text-cyan-neon",
  purple: "text-purple-plasma",
  green:  "text-green-signal",
  amber:  "text-amber-signal",
  red:    "text-red-signal",
} as const;

function formatValue(value: number, format: LiveMetricProps["format"]) {
  switch (format) {
    case "percent":  return `${value.toFixed(1)}%`;
    case "currency": return `₱${value.toLocaleString("en-PH")}`;
    case "duration": {
      const h = Math.floor(value / 60);
      const m = value % 60;
      return h > 0 ? `${h}h ${m}m` : `${m}m`;
    }
    default: return value.toLocaleString("en-PH");
  }
}

export function LiveMetric({
  label,
  value,
  unit,
  trend,
  color = "cyan",
  format = "number",
  live = false,
  className,
}: LiveMetricProps) {
  const motionValue = useMotionValue(0);
  const spring = useSpring(motionValue, { stiffness: 80, damping: 20 });
  const display = useTransform(spring, (v) => formatValue(Math.round(v), format));

  useEffect(() => {
    motionValue.set(value);
  }, [value, motionValue]);

  return (
    <div className={cn("flex flex-col gap-1", className)}>
      <div className="flex items-center gap-2">
        <span className="text-xs text-white/40 font-mono uppercase tracking-widest">
          {label}
        </span>
        {live && (
          <span className="relative flex h-1.5 w-1.5">
            <span className="absolute inline-flex h-full w-full rounded-full bg-green-signal opacity-75 animate-beacon" />
            <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-green-signal" />
          </span>
        )}
      </div>

      <div className="flex items-end gap-2">
        <motion.span
          className={cn("font-heading font-bold tabular-nums", colorMap[color], "text-3xl")}
        >
          {display}
        </motion.span>
        {unit && (
          <span className="text-white/30 text-sm font-mono mb-1">{unit}</span>
        )}
      </div>

      {trend !== undefined && (
        <span
          className={cn(
            "text-2xs font-mono",
            trend > 0  && "text-green-signal",
            trend < 0  && "text-red-signal",
            trend === 0 && "text-white/30"
          )}
        >
          {trend > 0 ? "↑" : trend < 0 ? "↓" : "→"}{" "}
          {Math.abs(trend).toFixed(1)}% vs yesterday
        </span>
      )}
    </div>
  );
}

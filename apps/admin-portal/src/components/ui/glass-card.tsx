"use client";
import { motion, type HTMLMotionProps } from "framer-motion";
import { cn } from "@/lib/design-system/cn";
import { variants } from "@/lib/design-system/tokens";

interface GlassCardProps extends HTMLMotionProps<"div"> {
  glow?: "cyan" | "purple" | "green" | "amber" | "red" | "none";
  size?: "sm" | "md" | "lg";
  /** Render a top-edge accent line in the glow color */
  accent?: boolean;
  /** Override padding — "none" removes all padding for table/map cards */
  padding?: "none";
}

const glowMap = {
  cyan:   "hover:shadow-glow-cyan   hover:border-glow-cyan",
  purple: "hover:shadow-glow-purple hover:border-glow-purple",
  green:  "hover:shadow-glow-green  hover:border-glow-green",
  amber:  "hover:shadow-glow-amber  hover:border-glow-amber",
  red:    "hover:shadow-glow-red    hover:border-glow-red",
  none:   "",
} as const;

const accentMap = {
  cyan:   "before:bg-cyan-neon",
  purple: "before:bg-purple-plasma",
  green:  "before:bg-green-signal",
  amber:  "before:bg-amber-signal",
  red:    "before:bg-red-signal",
  none:   "",
} as const;

const sizeMap = {
  sm: "glass-sm p-4",
  md: "glass p-6",
  lg: "glass-lg p-8",
} as const;

export function GlassCard({
  className,
  glow = "none",
  size = "md",
  accent = false,
  padding,
  children,
  ...props
}: GlassCardProps) {
  return (
    <motion.div
      variants={variants.glassCard}
      initial="rest"
      whileHover="hover"
      transition={variants.glassCard.hover}
      className={cn(
        "relative overflow-hidden transition-all duration-300",
        padding === "none" ? sizeMap[size].split(" ")[0] : sizeMap[size],
        glowMap[glow],
        // Top accent line
        accent && glow !== "none" && [
          "before:absolute before:inset-x-0 before:top-0 before:h-px before:content-['']",
          accentMap[glow],
          "before:opacity-70",
        ],
        className
      )}
      {...props}
    >
      {children}
    </motion.div>
  );
}

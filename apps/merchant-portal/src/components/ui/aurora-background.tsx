"use client";
/**
 * Aurora background — used on auth/landing pages.
 * Animated gradient blob behind the glassmorphism UI.
 * Inspired by Aceternity UI aurora pattern.
 */
import { cn } from "@/lib/design-system/cn";

interface AuroraBackgroundProps {
  className?: string;
  children?: React.ReactNode;
}

export function AuroraBackground({ className, children }: AuroraBackgroundProps) {
  return (
    <div className={cn("relative flex min-h-screen flex-col bg-canvas overflow-hidden", className)}>
      {/* Animated aurora blobs */}
      <div className="pointer-events-none absolute inset-0 overflow-hidden">
        <div
          className="absolute -top-40 -left-40 h-[600px] w-[600px] rounded-full opacity-20 blur-[120px] animate-aurora"
          style={{ background: "linear-gradient(135deg, #00E5FF, #A855F7)" }}
        />
        <div
          className="absolute -bottom-40 -right-20 h-[500px] w-[500px] rounded-full opacity-15 blur-[100px] animate-aurora"
          style={{
            background: "linear-gradient(135deg, #A855F7, #00FF88)",
            animationDelay: "-4s",
          }}
        />
        <div
          className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 h-[400px] w-[400px] rounded-full opacity-10 blur-[90px] animate-aurora"
          style={{
            background: "linear-gradient(135deg, #00FF88, #00E5FF)",
            animationDelay: "-2s",
          }}
        />
      </div>
      {/* Grid overlay */}
      <div
        className="pointer-events-none absolute inset-0 opacity-30"
        style={{
          backgroundImage:
            "linear-gradient(rgba(0,229,255,0.04) 1px, transparent 1px), linear-gradient(90deg, rgba(0,229,255,0.04) 1px, transparent 1px)",
          backgroundSize: "48px 48px",
        }}
      />
      <div className="relative z-10 flex flex-1 flex-col">{children}</div>
    </div>
  );
}

import { cn } from "@/lib/design-system/cn";

export type BadgeVariant = "cyan" | "purple" | "green" | "amber" | "red" | "muted";

interface NeonBadgeProps {
  variant?: BadgeVariant;
  pulse?: boolean;
  dot?: boolean;
  className?: string;
  children: React.ReactNode;
}

const variantStyles: Record<BadgeVariant, string> = {
  cyan:   "bg-cyan-surface   text-cyan-neon   border-cyan-neon/30   shadow-glow-sm-cyan",
  purple: "bg-purple-surface text-purple-plasma border-purple-plasma/30",
  green:  "bg-green-surface  text-green-signal border-green-signal/30 shadow-glow-sm-green",
  amber:  "bg-amber-surface  text-amber-signal border-amber-signal/30",
  red:    "bg-red-surface    text-red-signal   border-red-signal/30",
  muted:  "bg-glass-200      text-white/50     border-glass-border",
};

const dotStyles: Record<BadgeVariant, string> = {
  cyan:   "bg-cyan-neon",
  purple: "bg-purple-plasma",
  green:  "bg-green-signal",
  amber:  "bg-amber-signal",
  red:    "bg-red-signal",
  muted:  "bg-white/30",
};

export function NeonBadge({
  variant = "cyan",
  pulse = false,
  dot = false,
  className,
  children,
}: NeonBadgeProps) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5",
        "text-2xs font-medium font-mono uppercase tracking-wider",
        variantStyles[variant],
        className
      )}
    >
      {dot && (
        <span className="relative flex h-1.5 w-1.5 flex-shrink-0">
          <span
            className={cn(
              "absolute inline-flex h-full w-full rounded-full opacity-75",
              dotStyles[variant],
              pulse && "animate-beacon"
            )}
          />
          <span className={cn("relative inline-flex h-1.5 w-1.5 rounded-full", dotStyles[variant])} />
        </span>
      )}
      {children}
    </span>
  );
}

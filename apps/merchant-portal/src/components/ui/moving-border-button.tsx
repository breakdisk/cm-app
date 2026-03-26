"use client";
/**
 * Button with an animated gradient border that orbits around the edges.
 * Used for primary CTAs (Book Shipment, Create Campaign, Dispatch).
 */
import { useRef } from "react";
import { motion, useAnimationFrame, useMotionTemplate, useMotionValue, useTransform } from "framer-motion";
import { cn } from "@/lib/design-system/cn";

interface MovingBorderButtonProps {
  children: React.ReactNode;
  className?: string;
  containerClassName?: string;
  borderClassName?: string;
  duration?: number;
  onClick?: () => void;
  disabled?: boolean;
  size?: "sm" | "md" | "lg";
  variant?: "cyan" | "purple" | "green" | "amber";
}

const sizeMap = {
  sm:  { container: "h-8",  content: "px-4 text-xs" },
  md:  { container: "h-10", content: "px-6 text-sm" },
  lg:  { container: "h-12", content: "px-8 text-base" },
} as const;

const orbMap = {
  cyan:   "bg-[radial-gradient(circle_at_center,_#00E5FF_0%,_#A855F7_40%,_transparent_70%)]",
  purple: "bg-[radial-gradient(circle_at_center,_#A855F7_0%,_#00E5FF_40%,_transparent_70%)]",
  green:  "bg-[radial-gradient(circle_at_center,_#00FF88_0%,_#00E5FF_40%,_transparent_70%)]",
  amber:  "bg-[radial-gradient(circle_at_center,_#FFAB00_0%,_#FF3B5C_40%,_transparent_70%)]",
} as const;

export function MovingBorderButton({
  children,
  className,
  containerClassName,
  borderClassName,
  duration = 3000,
  onClick,
  disabled = false,
  size = "md",
  variant = "cyan",
}: MovingBorderButtonProps) {
  const pathRef = useRef<SVGRectElement>(null);
  const progress = useMotionValue<number>(0);

  useAnimationFrame((time) => {
    const length = pathRef.current?.getTotalLength?.() ?? 0;
    if (length) {
      const pxPerMillisecond = length / duration;
      progress.set((time * pxPerMillisecond) % length);
    }
  });

  const x = useTransform(progress, (val) => pathRef.current?.getPointAtLength(val)?.x ?? 0);
  const y = useTransform(progress, (val) => pathRef.current?.getPointAtLength(val)?.y ?? 0);

  const transform = useMotionTemplate`translateX(${x}px) translateY(${y}px) translateX(-50%) translateY(-50%)`;

  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={cn(
        "relative w-auto cursor-pointer overflow-hidden rounded-xl bg-transparent p-[1px] transition-opacity",
        sizeMap[size].container,
        disabled && "cursor-not-allowed opacity-50",
        containerClassName
      )}
    >
      {/* Moving gradient orb */}
      <div className="absolute inset-0">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          preserveAspectRatio="none"
          className="absolute h-full w-full"
          width="100%"
          height="100%"
        >
          <rect fill="none" width="100%" height="100%" rx="11" ry="11" ref={pathRef} />
        </svg>
        <motion.div
          style={{ position: "absolute", top: 0, left: 0, transform }}
          className={cn(
            "h-10 w-10 rounded-full opacity-80 blur-[2px]",
            orbMap[variant],
            borderClassName
          )}
        />
      </div>

      {/* Button content */}
      <span
        className={cn(
          "relative z-10 flex h-full w-full items-center justify-center gap-2 rounded-xl",
          "bg-canvas-100 font-medium text-white",
          sizeMap[size].content,
          "hover:bg-canvas-200 transition-colors",
          className
        )}
      >
        {children}
      </span>
    </button>
  );
}

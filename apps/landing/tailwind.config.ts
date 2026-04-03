import type { Config } from "tailwindcss";
import { fontFamily } from "tailwindcss/defaultTheme";

const config: Config = {
  darkMode: ["class"],
  content: ["./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        canvas: {
          DEFAULT: "#050810",
          50:  "#0a0f1e",
          100: "#0d1422",
          200: "#111827",
          300: "#1a2235",
          400: "#1e2a40",
        },
        cyan:   { neon: "#00E5FF", glow: "#00B8D9", dim: "#007A91" },
        purple: { plasma: "#A855F7", glow: "#7C3AED", dim: "#5B21B6" },
        green:  { signal: "#00FF88", glow: "#00CC6A", dim: "#008542" },
        amber:  { signal: "#FFAB00", glow: "#FF8F00" },
        glass: {
          100: "rgba(255,255,255,0.04)",
          200: "rgba(255,255,255,0.06)",
          300: "rgba(255,255,255,0.08)",
          400: "rgba(255,255,255,0.12)",
          border: "rgba(255,255,255,0.08)",
          "border-bright": "rgba(255,255,255,0.16)",
        },
      },
      fontFamily: {
        sans:    ["var(--font-geist)", "Inter", ...fontFamily.sans],
        heading: ["var(--font-space)", "Geist", ...fontFamily.sans],
        mono:    ["var(--font-mono)", "Fira Code", ...fontFamily.mono],
      },
      boxShadow: {
        "glow-cyan":   "0 0 20px rgba(0,229,255,0.35), 0 0 60px rgba(0,229,255,0.12)",
        "glow-purple": "0 0 20px rgba(168,85,247,0.35), 0 0 60px rgba(168,85,247,0.12)",
        "glow-green":  "0 0 20px rgba(0,255,136,0.35), 0 0 60px rgba(0,255,136,0.12)",
        "glow-amber":  "0 0 20px rgba(255,171,0,0.35),  0 0 60px rgba(255,171,0,0.12)",
        "glass":       "0 4px 24px rgba(0,0,0,0.4), inset 0 1px 0 rgba(255,255,255,0.06)",
        "glass-lg":    "0 8px 40px rgba(0,0,0,0.5), inset 0 1px 0 rgba(255,255,255,0.08)",
      },
      backgroundImage: {
        "grid-pattern": "linear-gradient(rgba(0,229,255,0.04) 1px, transparent 1px), linear-gradient(90deg, rgba(0,229,255,0.04) 1px, transparent 1px)",
        "dot-pattern":  "radial-gradient(rgba(0,229,255,0.10) 1px, transparent 1px)",
        "gradient-brand": "linear-gradient(135deg, #00E5FF 0%, #A855F7 50%, #00FF88 100%)",
      },
      backgroundSize: {
        "grid-md": "48px 48px",
        "dot-sm":  "20px 20px",
      },
      keyframes: {
        "pulse-neon": { "0%,100%": { opacity: "1" }, "50%": { opacity: "0.35" } },
        "float":      { "0%,100%": { transform: "translateY(0)" }, "50%": { transform: "translateY(-10px)" } },
        "shimmer":    { "0%": { backgroundPosition: "-200% center" }, "100%": { backgroundPosition: "200% center" } },
        "aurora":     { "0%,100%": { backgroundPosition: "0% 50%" }, "50%": { backgroundPosition: "100% 50%" } },
        "beacon":     { "0%": { transform: "scale(1)", opacity: "0.8" }, "100%": { transform: "scale(2.5)", opacity: "0" } },
        "slide-up":   { "0%": { transform: "translateY(24px)", opacity: "0" }, "100%": { transform: "translateY(0)", opacity: "1" } },
        "ticker":     { "0%": { transform: "translateX(0)" }, "100%": { transform: "translateX(-50%)" } },
      },
      animation: {
        "pulse-neon": "pulse-neon 2.5s ease-in-out infinite",
        "float":      "float 5s ease-in-out infinite",
        "shimmer":    "shimmer 2.5s linear infinite",
        "aurora":     "aurora 10s ease-in-out infinite",
        "beacon":     "beacon 1.8s ease-out infinite",
        "slide-up":   "slide-up 0.6s cubic-bezier(0.16,1,0.3,1) forwards",
        "ticker":     "ticker 30s linear infinite",
      },
    },
  },
  plugins: [require("tailwindcss-animate")],
};

export default config;

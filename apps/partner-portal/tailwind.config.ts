import type { Config } from "tailwindcss";
import { fontFamily } from "tailwindcss/defaultTheme";

const config: Config = {
  darkMode: ["class"],
  content: [
    "./src/**/*.{ts,tsx}",
    "../../packages/ui/src/**/*.{ts,tsx}",
  ],
  theme: {
    extend: {
      // ── Color palette ────────────────────────────────────────
      colors: {
        // Base canvas — near-black with a very subtle blue tint
        canvas: {
          DEFAULT: "#050810",
          50:  "#0a0f1e",
          100: "#0d1422",
          200: "#111827",
          300: "#1a2235",
          400: "#1e2a40",
        },
        // Neon accents
        cyan: {
          neon:   "#00E5FF",
          glow:   "#00B8D9",
          dim:    "#007A91",
          surface:"rgba(0, 229, 255, 0.06)",
        },
        purple: {
          plasma: "#A855F7",
          glow:   "#7C3AED",
          dim:    "#5B21B6",
          surface:"rgba(168, 85, 247, 0.06)",
        },
        green: {
          signal: "#00FF88",
          glow:   "#00CC6A",
          dim:    "#008542",
          surface:"rgba(0, 255, 136, 0.06)",
        },
        amber: {
          signal: "#FFAB00",
          glow:   "#FF8F00",
          dim:    "#B35F00",
          surface:"rgba(255, 171, 0, 0.06)",
        },
        red: {
          signal: "#FF3B5C",
          glow:   "#E0002B",
          surface:"rgba(255, 59, 92, 0.06)",
        },
        // Glass surface layering
        glass: {
          100: "rgba(255, 255, 255, 0.04)",
          200: "rgba(255, 255, 255, 0.06)",
          300: "rgba(255, 255, 255, 0.08)",
          400: "rgba(255, 255, 255, 0.12)",
          border: "rgba(255, 255, 255, 0.08)",
          "border-bright": "rgba(255, 255, 255, 0.16)",
        },
      },

      // ── Typography ───────────────────────────────────────────
      fontFamily: {
        sans:  ["Geist", "Inter", ...fontFamily.sans],
        heading: ["Space Grotesk", "Geist", ...fontFamily.sans],
        mono:  ["JetBrains Mono", "Fira Code", ...fontFamily.mono],
      },
      fontSize: {
        "2xs": ["0.65rem", { lineHeight: "1rem" }],
      },

      // ── Spacing ──────────────────────────────────────────────
      spacing: {
        "18": "4.5rem",
        "22": "5.5rem",
        "112": "28rem",
        "128": "32rem",
      },

      // ── Border radius ────────────────────────────────────────
      borderRadius: {
        "4xl": "2rem",
        "5xl": "2.5rem",
      },

      // ── Box shadows — neon glow system ───────────────────────
      boxShadow: {
        "glow-cyan":   "0 0 20px rgba(0, 229, 255, 0.35), 0 0 40px rgba(0, 229, 255, 0.15)",
        "glow-purple": "0 0 20px rgba(168, 85, 247, 0.35), 0 0 40px rgba(168, 85, 247, 0.15)",
        "glow-green":  "0 0 20px rgba(0, 255, 136, 0.35), 0 0 40px rgba(0, 255, 136, 0.15)",
        "glow-amber":  "0 0 20px rgba(255, 171, 0, 0.35), 0 0 40px rgba(255, 171, 0, 0.15)",
        "glow-red":    "0 0 20px rgba(255, 59, 92, 0.35), 0 0 40px rgba(255, 59, 92, 0.15)",
        "glow-sm-cyan":  "0 0 8px rgba(0, 229, 255, 0.5)",
        "glow-sm-green": "0 0 8px rgba(0, 255, 136, 0.5)",
        "glass":  "0 4px 24px rgba(0, 0, 0, 0.4), inset 0 1px 0 rgba(255,255,255,0.06)",
        "glass-lg": "0 8px 40px rgba(0, 0, 0, 0.5), inset 0 1px 0 rgba(255,255,255,0.08)",
      },

      // ── Background images / gradients ────────────────────────
      backgroundImage: {
        "grid-pattern":
          "linear-gradient(rgba(0,229,255,0.04) 1px, transparent 1px), linear-gradient(90deg, rgba(0,229,255,0.04) 1px, transparent 1px)",
        "dot-pattern":
          "radial-gradient(rgba(0,229,255,0.12) 1px, transparent 1px)",
        "gradient-radial": "radial-gradient(var(--tw-gradient-stops))",
        "gradient-conic":  "conic-gradient(from 180deg at 50% 50%, var(--tw-gradient-stops))",
        "gradient-brand":
          "linear-gradient(135deg, #00E5FF 0%, #A855F7 50%, #00FF88 100%)",
        "gradient-dispatch":
          "linear-gradient(135deg, #00E5FF 0%, #007A91 100%)",
        "gradient-alert":
          "linear-gradient(135deg, #FF3B5C 0%, #FFAB00 100%)",
      },

      backgroundSize: {
        "grid-sm": "24px 24px",
        "grid-md": "48px 48px",
        "dot-sm":  "16px 16px",
      },

      // ── Keyframes + animation ────────────────────────────────
      keyframes: {
        "pulse-neon": {
          "0%, 100%": { opacity: "1" },
          "50%":       { opacity: "0.4" },
        },
        "shimmer": {
          "0%":   { backgroundPosition: "-200% center" },
          "100%": { backgroundPosition: "200% center" },
        },
        "float": {
          "0%, 100%": { transform: "translateY(0px)" },
          "50%":       { transform: "translateY(-8px)" },
        },
        "scan-line": {
          "0%":   { transform: "translateY(-100%)" },
          "100%": { transform: "translateY(100vh)" },
        },
        "beacon": {
          "0%":   { transform: "scale(1)",   opacity: "0.8" },
          "100%": { transform: "scale(2.5)", opacity: "0" },
        },
        "aurora": {
          "0%, 100%": { backgroundPosition: "0% 50%" },
          "50%":       { backgroundPosition: "100% 50%" },
        },
      },
      animation: {
        "pulse-neon": "pulse-neon 2s ease-in-out infinite",
        "shimmer":    "shimmer 2.5s linear infinite",
        "float":      "float 4s ease-in-out infinite",
        "scan-line":  "scan-line 3s linear infinite",
        "beacon":     "beacon 1.5s ease-out infinite",
        "aurora":     "aurora 8s ease-in-out infinite",
      },

      // ── Backdrop blur ────────────────────────────────────────
      backdropBlur: {
        xs: "2px",
        "4xl": "72px",
      },
    },
  },
  plugins: [
    require("tailwindcss-animate"),
    require("@tailwindcss/typography"),
    require("@tailwindcss/container-queries"),
  ],
};

export default config;

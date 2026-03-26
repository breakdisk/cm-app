/**
 * LogisticOS shared design tokens.
 * Single source of truth for the dark-first glassmorphism theme.
 * Mirrors tailwind.config.ts for use in JS contexts (Three.js, Framer Motion, canvas).
 */

export const colors = {
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
  amber:  { signal: "#FFAB00", glow: "#FF8F00", dim: "#B35F00" },
  red:    { signal: "#FF3B5C", glow: "#E0002B", dim: "#9B001E" },
  glass:  {
    border:  "rgba(255,255,255,0.08)",
    surface: "rgba(255,255,255,0.04)",
    hover:   "rgba(255,255,255,0.07)",
    active:  "rgba(255,255,255,0.10)",
  },
} as const;

export const statusColors: Record<string, { text: string; glow: string; bg: string }> = {
  pending:           { text: colors.amber.signal, glow: colors.amber.glow,   bg: "rgba(255,171,0,0.08)"  },
  confirmed:         { text: colors.cyan.neon,    glow: colors.cyan.glow,    bg: "rgba(0,229,255,0.08)"  },
  pickup_assigned:   { text: colors.cyan.neon,    glow: colors.cyan.glow,    bg: "rgba(0,229,255,0.08)"  },
  picked_up:         { text: colors.cyan.neon,    glow: colors.cyan.glow,    bg: "rgba(0,229,255,0.08)"  },
  in_transit:        { text: colors.purple.plasma, glow: colors.purple.glow, bg: "rgba(168,85,247,0.08)" },
  at_hub:            { text: colors.purple.plasma, glow: colors.purple.glow, bg: "rgba(168,85,247,0.08)" },
  out_for_delivery:  { text: colors.cyan.neon,    glow: colors.cyan.glow,    bg: "rgba(0,229,255,0.08)"  },
  delivered:         { text: colors.green.signal, glow: colors.green.glow,   bg: "rgba(0,255,136,0.08)"  },
  failed:            { text: colors.red.signal,   glow: colors.red.glow,     bg: "rgba(255,59,92,0.08)"  },
  returned:          { text: colors.amber.signal, glow: colors.amber.glow,   bg: "rgba(255,171,0,0.08)"  },
  cancelled:         { text: colors.red.signal,   glow: colors.red.glow,     bg: "rgba(255,59,92,0.08)"  },
};

export const typography = {
  fontSans:    "var(--font-sans)",
  fontHeading: "var(--font-heading)",
  fontMono:    "var(--font-mono)",
};

export const easing = {
  springOut: [0.16, 1, 0.3, 1] as const,
  smooth:    [0.4, 0, 0.2, 1] as const,
};

export const glass = {
  card: {
    background:   "rgba(255,255,255,0.04)",
    border:       "1px solid rgba(255,255,255,0.08)",
    borderRadius: "12px",
    backdropFilter: "blur(16px)",
  },
  cardHover: {
    background: "rgba(255,255,255,0.07)",
    border:     "1px solid rgba(255,255,255,0.12)",
  },
} as const;

/** Framer Motion shared variants */
export const variants = {
  staggerContainer: {
    hidden: {},
    visible: { transition: { staggerChildren: 0.07 } },
  },
  fadeInUp: {
    hidden:  { opacity: 0, y: 16 },
    visible: { opacity: 1, y: 0, transition: { duration: 0.45, ease: easing.springOut } },
  },
  fadeIn: {
    hidden:  { opacity: 0 },
    visible: { opacity: 1, transition: { duration: 0.3 } },
  },
  scaleIn: {
    hidden:  { opacity: 0, scale: 0.95 },
    visible: { opacity: 1, scale: 1, transition: { duration: 0.3, ease: easing.springOut } },
  },
} as const;

export type StatusKey = keyof typeof statusColors;

/**
 * Design tokens — Admin Portal (mirrors merchant-portal tokens).
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
  red:    { signal: "#FF3B5C", glow: "#E0002B" },
} as const;

export const statusColors: Record<string, { text: string; glow: string; bg: string }> = {
  pending:            { text: colors.amber.signal, glow: colors.amber.glow, bg: "rgba(255,171,0,0.08)" },
  confirmed:          { text: colors.cyan.neon,    glow: colors.cyan.glow,  bg: "rgba(0,229,255,0.08)" },
  pickup_assigned:    { text: colors.cyan.neon,    glow: colors.cyan.glow,  bg: "rgba(0,229,255,0.08)" },
  picked_up:          { text: colors.cyan.neon,    glow: colors.cyan.glow,  bg: "rgba(0,229,255,0.08)" },
  in_transit:         { text: colors.purple.plasma, glow: colors.purple.glow, bg: "rgba(168,85,247,0.08)" },
  at_hub:             { text: colors.purple.plasma, glow: colors.purple.glow, bg: "rgba(168,85,247,0.08)" },
  out_for_delivery:   { text: colors.green.signal, glow: colors.green.glow, bg: "rgba(0,255,136,0.08)" },
  delivered:          { text: colors.green.signal, glow: colors.green.glow, bg: "rgba(0,255,136,0.08)" },
  failed:             { text: colors.red.signal,   glow: colors.red.glow,   bg: "rgba(255,59,92,0.08)" },
  cancelled:          { text: "#4B5563",            glow: "#374151",          bg: "rgba(75,85,99,0.08)" },
  returned:           { text: colors.amber.signal, glow: colors.amber.glow, bg: "rgba(255,171,0,0.08)" },
};

export const spring = {
  snappy:   { type: "spring", stiffness: 400, damping: 30 },
  gentle:   { type: "spring", stiffness: 120, damping: 20 },
  slow:     { type: "spring", stiffness: 60,  damping: 15 },
  page:     { type: "tween", ease: [0.16, 1, 0.3, 1], duration: 0.45 },
} as const;

export const variants = {
  fadeInUp: {
    hidden:  { opacity: 0, y: 16 },
    visible: { opacity: 1, y: 0, transition: spring.snappy },
  },
  fadeIn: {
    hidden:  { opacity: 0 },
    visible: { opacity: 1, transition: { duration: 0.4 } },
  },
  staggerContainer: {
    hidden:  {},
    visible: { transition: { staggerChildren: 0.08 } },
  },
  glassCard: {
    rest:  { scale: 1,    boxShadow: "0 4px 24px rgba(0,0,0,0.4)" },
    hover: { scale: 1.01, boxShadow: "0 0 20px rgba(0,229,255,0.2), 0 8px 40px rgba(0,0,0,0.5)" },
  },
} as const;

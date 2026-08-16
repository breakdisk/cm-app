/** Mirrors the platform design tokens. Dark is the canvas, not a mode. */
export const theme = {
  canvas:  "#050810",
  surface: "rgba(255,255,255,0.045)",
  border:  "rgba(255,255,255,0.10)",
  text:    "#FFFFFF",
  muted:   "rgba(255,255,255,0.55)",
  faint:   "rgba(255,255,255,0.36)",
  cyan:    "#00E5FF",
  purple:  "#A855F7",
  green:   "#00FF88",
  amber:   "#FFAB00",
  red:     "#FF3B5C",
  radius:  { sm: 10, md: 14, lg: 18 },
  /** Spring-out. Everything that changes state animates. */
  easing:  [0.16, 1, 0.3, 1] as const,
} as const;

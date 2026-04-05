export const COLORS = {
  // Base
  CANVAS: '#050810',
  SURFACE: '#0f1419',
  BORDER: '#1a1f2e',

  // Accent palette (neon)
  CYAN: '#00E5FF',
  CYAN_DARK: '#00A8CC',
  PURPLE: '#A855F7',
  GREEN: '#00FF88',
  AMBER: '#FFAB00',
  RED: '#FF4444',

  // Semantic
  SUCCESS: '#00FF88',
  WARNING: '#FFAB00',
  ERROR: '#FF4444',
  INFO: '#00E5FF',

  // Text
  TEXT_PRIMARY: '#FFFFFF',
  TEXT_SECONDARY: '#A0AEC0',
  TEXT_TERTIARY: '#64748B',

  // Glass
  GLASS: 'rgba(255, 255, 255, 0.05)',
  GLASS_HOVER: 'rgba(255, 255, 255, 0.08)',
} as const;

export const SHADOWS = {
  GLOW_CYAN: '0 0 20px rgba(0, 229, 255, 0.3)',
  GLOW_PURPLE: '0 0 20px rgba(168, 85, 247, 0.3)',
  GLOW_GREEN: '0 0 20px rgba(0, 255, 136, 0.3)',
} as const;

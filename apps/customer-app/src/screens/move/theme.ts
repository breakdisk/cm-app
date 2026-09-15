/**
 * Tokens for the Move app.
 *
 * The handoff's cyan (#2dd4e8) and near-black ground are mapped onto the
 * platform palette in CLAUDE.md (decided 2026-09-12). Its contrast floors are
 * kept: muted text never drops below .42 alpha at 11px or .6 at 13px.
 *
 * Headings: the design specifies Barlow Condensed. It is not bundled in this
 * app, so headings use the platform's condensed sans instead — split weight
 * (light over bold) as designed.
 */
import { Platform } from 'react-native';

export const M = {
  ground:           '#050810',
  ink:              '#F2F6FA',
  accent:           '#00E5FF',
  accentGrad:       ['#6CF1FF', '#00C9E0'] as [string, string],
  accentInk:        '#04222A',
  accentText:       '#D6FBFF',
  accentTint:       'rgba(0,229,255,0.08)',
  accentTintStrong: 'rgba(0,229,255,0.14)',
  accentBorder:     'rgba(0,229,255,0.30)',
  amber:            '#FFAB00',
  amberText:        '#FFC65C',
  amberTint:        'rgba(255,171,0,0.10)',
  amberBorder:      'rgba(255,171,0,0.30)',
  green:            '#00FF88',
  penalty:          '#FF8C8C',
  penaltyGrad:      ['#FF9D9D', '#F26A6A'] as [string, string],
  penaltyInk:       '#2A0606',
  penaltyTint:      'rgba(255,140,140,0.10)',
  penaltyBorder:    'rgba(255,140,140,0.30)',
  panel:            'rgba(255,255,255,0.045)',
  panelStrong:      'rgba(255,255,255,0.07)',
  hairline:         'rgba(255,255,255,0.09)',
  hairlineStrong:   'rgba(255,255,255,0.13)',
  /** 11px uppercase labels. */
  label:            'rgba(242,246,250,0.48)',
  /** 13px body copy. */
  muted:            'rgba(242,246,250,0.62)',
  /** 11px notes under a value. */
  faint:            'rgba(242,246,250,0.46)',
  sheet:            'rgba(12,16,22,0.96)',
} as const;

export const HEADING = Platform.select({
  android: 'sans-serif-condensed',
  ios: 'AvenirNextCondensed-DemiBold',
  default: undefined,
});

export const HEADING_LIGHT = Platform.select({
  android: 'sans-serif-condensed-light',
  ios: 'AvenirNextCondensed-UltraLight',
  default: undefined,
});

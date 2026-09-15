/**
 * Sign-in styling (phone + code, profile, identity) for both builds.
 *
 * The default build keeps its original look, value for value. The Move build
 * takes the Move tokens, so the first screens a mover sees match the rest of
 * that app instead of the older purple-gradient onboarding.
 */
import type { TextStyle, ViewStyle } from 'react-native';
import { IS_MOVE_APP } from '../../config/variant';
import { HEADING, M } from '../move/theme';

type Grad = [string, string];

export interface AuthTheme {
  canvas: string;
  ink: string;
  muted: string;
  faint: string;
  placeholder: string;
  accent: string;
  /** Second brand colour — purple in the default build, the accent in Move. */
  secondary: string;
  glass: string;
  border: string;
  inputBg: string;
  success: string;
  amber: string;
  danger: string;
  /** Send / continue. */
  primaryGrad: Grad;
  /** Verify / submit. */
  confirmGrad: Grad;
  /** Soft wash behind each screen's heading. */
  wash: { phone: Grad; profile: Grad; kyc: Grad };
  progress: { profile: string; kyc: string };
  heading: TextStyle;
  headingSemi: TextStyle;
  mono: TextStyle;
  button: ViewStyle;
  buttonText: TextStyle;
}

const CLASSIC: AuthTheme = {
  canvas:      '#050810',
  ink:         '#FFF',
  muted:       'rgba(255,255,255,0.4)',
  faint:       'rgba(255,255,255,0.3)',
  placeholder: 'rgba(255,255,255,0.2)',
  accent:      '#00E5FF',
  secondary:   '#A855F7',
  glass:       'rgba(255,255,255,0.04)',
  border:      'rgba(255,255,255,0.08)',
  inputBg:     'rgba(255,255,255,0.03)',
  success:     '#00FF88',
  amber:       '#FFAB00',
  danger:      '#FF3B5C',
  primaryGrad: ['#00E5FF', '#A855F7'],
  confirmGrad: ['#00FF88', '#00E5FF'],
  wash: {
    phone:   ['rgba(0,229,255,0.10)', 'transparent'],
    profile: ['rgba(168,85,247,0.10)', 'transparent'],
    kyc:     ['rgba(0,255,136,0.08)', 'transparent'],
  },
  progress:    { profile: '#A855F7', kyc: '#00FF88' },
  heading:     { fontFamily: 'SpaceGrotesk-Bold' },
  headingSemi: { fontFamily: 'SpaceGrotesk-SemiBold' },
  mono:        { fontFamily: 'JetBrainsMono-Regular' },
  button:      { borderRadius: 14, paddingVertical: 15, alignItems: 'center' },
  buttonText:  { fontFamily: 'SpaceGrotesk-SemiBold', color: '#050810' },
};

const MOVE_WASH: Grad = ['rgba(0,229,255,0.10)', 'transparent'];

const MOVE: AuthTheme = {
  canvas:      M.ground,
  ink:         M.ink,
  muted:       M.muted,
  faint:       M.faint,
  placeholder: M.faint,
  accent:      M.accent,
  secondary:   M.accent,
  glass:       M.panel,
  border:      M.hairlineStrong,
  inputBg:     'rgba(255,255,255,0.05)',
  success:     M.green,
  amber:       M.amber,
  danger:      M.penalty,
  primaryGrad: M.accentGrad,
  confirmGrad: M.accentGrad,
  wash:        { phone: MOVE_WASH, profile: MOVE_WASH, kyc: MOVE_WASH },
  progress:    { profile: M.accent, kyc: M.accent },
  heading:     { fontFamily: HEADING, fontWeight: '700' },
  headingSemi: { fontFamily: HEADING, fontWeight: '700' },
  // The Move app sets codes in the system face, as the rest of that app does.
  mono:        {},
  button:      { borderRadius: 18, height: 60, alignItems: 'center', justifyContent: 'center', paddingHorizontal: 16 },
  buttonText:  { fontWeight: '700', letterSpacing: 1.3, color: M.accentInk },
};

export const A: AuthTheme = IS_MOVE_APP ? MOVE : CLASSIC;

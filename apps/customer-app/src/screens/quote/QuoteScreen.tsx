/**
 * QuoteScreen — Customer App
 *
 * Two entry modes:
 *   a) Standalone quote — accessible from HomeScreen quick-actions
 *   b) Pre-fill mode    — opened with initial L×W×H from AR measurement
 *
 * Flow:
 *   AR Measure (optional) → select origin + mode → select box size → PH province
 *   → Quote result → "Book Now" (pre-fills BookingScreen)
 *
 * Design language: dark glassmorphism, neon accents matching CLAUDE.md spec.
 */

import React, { useState, useEffect, useCallback, useRef } from 'react';
import {
  View, Text, ScrollView, TouchableOpacity, TextInput, Alert,
  ActivityIndicator, Animated, Platform, StyleSheet, Dimensions,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { MaterialIcons } from '@expo/vector-icons';
import { LinearGradient } from 'expo-linear-gradient';

import { ArMeasurementModule } from '../../../modules/ar-measurement/src';
import type { BoxDimensions } from '../../../modules/ar-measurement/src';
import {
  BOX_SIZES, SEA_CARGO, AIR_ZONES, computeQuote, computeCbm,
  matchToStandardSize, resolveProvince, fmtAmount,
  type QuoteResult, type BoxSize,
} from '../../lib/quote-engine';

// ── Design tokens ──────────────────────────────────────────────────────────────
const CANVAS  = '#050810';
const CYAN    = '#00E5FF';
const PURPLE  = '#A855F7';
const GREEN   = '#00FF88';
const AMBER   = '#FFAB00';
const GLASS   = 'rgba(255,255,255,0.04)';
const BORDER  = 'rgba(255,255,255,0.08)';
const TEXT    = '#e2e8f0';
const MUTED   = 'rgba(255,255,255,0.35)';

const COMPONENT_COLOR: Record<string, string> = {
  sea: CYAN, air: PURPLE, ph_delivery: GREEN,
};

const STEP_LABELS = ['Mode', 'Origin', 'Box', 'Province', 'Quote'];
const { width: SCREEN_W } = Dimensions.get('window');

// ── Step indicator ─────────────────────────────────────────────────────────────
function StepBar({ step }: { step: number }) {
  return (
    <View style={styles.stepRow}>
      {STEP_LABELS.map((label, i) => (
        <React.Fragment key={label}>
          <View style={styles.stepItem}>
            <View style={[styles.stepDot, i <= step && styles.stepDotActive]}>
              {i < step
                ? <MaterialIcons name="check" size={10} color={CANVAS} />
                : <Text style={[styles.stepNum, i === step && { color: CANVAS }]}>{i + 1}</Text>
              }
            </View>
            <Text style={[styles.stepLabel, i === step && { color: CYAN }]} numberOfLines={1}>{label}</Text>
          </View>
          {i < STEP_LABELS.length - 1 && (
            <View style={[styles.stepLine, i < step && styles.stepLineActive]} />
          )}
        </React.Fragment>
      ))}
    </View>
  );
}

// ── Box size card ──────────────────────────────────────────────────────────────
function BoxCard({ size, selected, onPress }: { size: BoxSize; selected: boolean; onPress: () => void }) {
  return (
    <TouchableOpacity
      onPress={onPress}
      style={[styles.boxCard, selected && styles.boxCardSelected]}
      activeOpacity={0.8}
    >
      {/* Simple 3D box icon */}
      <View style={styles.boxIcon}>
        <View style={[styles.boxFace, { width: 28, height: 20, borderColor: selected ? CYAN : BORDER }]} />
        <View style={[styles.boxTop,  { borderColor: selected ? CYAN : BORDER }]} />
      </View>
      <Text style={[styles.boxName, selected && { color: CYAN }]}>{size.name}</Text>
      <Text style={styles.boxDims}>{size.dimensions}</Text>
      <Text style={styles.boxCbm}>{size.cbm} m³</Text>
      {selected && <View style={styles.boxCheck}><MaterialIcons name="check" size={10} color={CANVAS} /></View>}
    </TouchableOpacity>
  );
}

// ── Quote line row ─────────────────────────────────────────────────────────────
function QuoteLine({ label, amount, currency, note, component }: {
  label: string; amount: number; currency: string; note?: string; component: string;
}) {
  const color = COMPONENT_COLOR[component] ?? TEXT;
  return (
    <View style={styles.quoteLine}>
      <View style={{ flex: 1, marginRight: 8 }}>
        <Text style={[styles.quoteLineLabel, { color }]}>{label}</Text>
        {note ? <Text style={styles.quoteLineNote} numberOfLines={2}>{note}</Text> : null}
      </View>
      <Text style={styles.quoteLineAmount}>{fmtAmount(amount, currency)}</Text>
    </View>
  );
}

// ── Main screen ────────────────────────────────────────────────────────────────

export function QuoteScreen({ navigation, route }: any) {
  const insets = useSafeAreaInsets();

  // If opened via deep-link / "Book Now" flow, prefill dimensions from AR
  const initDims: BoxDimensions | null = route?.params?.arDimensions ?? null;

  const [step, setStep]             = useState(0);
  const [mode, setMode]             = useState<'sea' | 'air'>('sea');
  const [originKey, setOriginKey]   = useState(SEA_CARGO[0].origin);
  const [sizeId, setSizeId]         = useState('jumbo');
  const [qty, setQty]               = useState(1);
  const [weightKg, setWeightKg]     = useState(20);
  const [province, setProvince]     = useState('');
  const [result, setResult]         = useState<QuoteResult | null>(null);
  const [arLoading, setArLoading]   = useState(false);
  const [arAvailable, setArAvailable] = useState(false);
  const [measuredDims, setMeasuredDims] = useState<BoxDimensions | null>(initDims);

  const fadeAnim = useRef(new Animated.Value(1)).current;

  // Check AR availability on mount
  useEffect(() => {
    ArMeasurementModule.isAvailable()
      .then(setArAvailable)
      .catch(() => setArAvailable(false));
  }, []);

  // If dims were provided (AR result), auto-match to box size
  useEffect(() => {
    if (measuredDims) {
      const match = matchToStandardSize(measuredDims.length, measuredDims.width, measuredDims.height);
      if (match) setSizeId(match.id);
    }
  }, [measuredDims]);

  // Update default origin when mode changes
  useEffect(() => {
    setOriginKey(mode === 'sea' ? SEA_CARGO[0].origin : AIR_ZONES[0].zone_name);
  }, [mode]);

  const animateTransition = useCallback((fn: () => void) => {
    Animated.sequence([
      Animated.timing(fadeAnim, { toValue: 0, duration: 120, useNativeDriver: true }),
      Animated.timing(fadeAnim, { toValue: 1, duration: 200, useNativeDriver: true }),
    ]).start();
    fn();
  }, [fadeAnim]);

  const handleNext = useCallback(() => {
    if (step === 3) {
      const size = BOX_SIZES.find(s => s.id === sizeId);
      const dims: [number, number, number] = measuredDims
        ? [measuredDims.length, measuredDims.width, measuredDims.height]
        : (size ? size.dims_cm : [0, 0, 0]);
      const r = computeQuote(mode, originKey, sizeId, qty, weightKg, dims, province);
      setResult(r);
      animateTransition(() => setStep(4));
    } else {
      animateTransition(() => setStep(s => s + 1));
    }
  }, [step, mode, originKey, sizeId, qty, weightKg, province, measuredDims, animateTransition]);

  const handleBack = useCallback(() => {
    if (step === 0) {
      navigation.goBack();
    } else {
      animateTransition(() => { setStep(s => s - 1); if (step === 4) setResult(null); });
    }
  }, [step, navigation, animateTransition]);

  const handleARMeasure = useCallback(async () => {
    setArLoading(true);
    try {
      const dims = await ArMeasurementModule.measureBox();
      setMeasuredDims(dims);
      const match = matchToStandardSize(dims.length, dims.width, dims.height);
      if (match) setSizeId(match.id);
      Alert.alert(
        'Box Measured ✓',
        `L ${dims.length} cm × W ${dims.width} cm × H ${dims.height} cm\nMatched to: ${match?.name ?? 'Custom'}\nConfidence: ${Math.round(dims.confidence * 100)}%`,
        [{ text: 'OK' }],
      );
    } catch (err: any) {
      if (err?.code !== 'USER_CANCELLED') {
        Alert.alert('Measurement failed', err?.message ?? 'Please try again or enter dimensions manually.');
      }
    } finally {
      setArLoading(false);
    }
  }, []);

  const handleBookNow = useCallback(() => {
    const size = BOX_SIZES.find(s => s.id === sizeId);
    navigation.navigate('Tabs', {
      screen: 'Book',
      params: {
        prefill: {
          length_cm: measuredDims?.length ?? size?.dims_cm[0] ?? 0,
          width_cm:  measuredDims?.width  ?? size?.dims_cm[1] ?? 0,
          height_cm: measuredDims?.height ?? size?.dims_cm[2] ?? 0,
          weight_kg: weightKg,
          box_size:  sizeId,
          freight_mode: mode,
          origin: originKey,
          estimated_total: result?.total_origin_currency,
          currency: result?.origin_currency,
        },
      },
    });
  }, [navigation, sizeId, measuredDims, weightKg, mode, originKey, result]);

  const origins = mode === 'sea' ? SEA_CARGO.map(r => r.origin) : AIR_ZONES.map(z => z.zone_name);
  const resolvedZone = resolveProvince(province);
  const selectedSize = BOX_SIZES.find(s => s.id === sizeId);
  const cbm = measuredDims
    ? computeCbm(measuredDims.length, measuredDims.width, measuredDims.height)
    : selectedSize?.cbm ?? 0;

  return (
    <View style={[styles.container, { paddingTop: insets.top }]}>
      {/* Header */}
      <View style={styles.header}>
        <TouchableOpacity onPress={handleBack} style={styles.backBtn} hitSlop={{ top: 12, bottom: 12, left: 12, right: 12 }}>
          <MaterialIcons name="arrow-back" size={20} color={TEXT} />
        </TouchableOpacity>
        <View style={{ flex: 1 }}>
          <Text style={styles.title}>Box Quote</Text>
          <Text style={styles.subtitle}>Instant price estimate</Text>
        </View>
        {measuredDims && (
          <View style={styles.arBadge}>
            <MaterialIcons name="view-in-ar" size={12} color={CYAN} />
            <Text style={styles.arBadgeText}>AR</Text>
          </View>
        )}
      </View>

      <StepBar step={step} />

      <ScrollView
        style={{ flex: 1 }}
        contentContainerStyle={styles.scrollContent}
        keyboardShouldPersistTaps="handled"
        showsVerticalScrollIndicator={false}
      >
        <Animated.View style={{ opacity: fadeAnim }}>

          {/* ── Step 0: Mode ─────────────────────────────────────────── */}
          {step === 0 && (
            <View style={styles.stepContent}>
              <Text style={styles.stepTitle}>How are you sending?</Text>
              <Text style={styles.stepSub}>Choose your preferred shipping method.</Text>

              <View style={styles.modeRow}>
                {(['sea', 'air'] as const).map(m => (
                  <TouchableOpacity
                    key={m}
                    onPress={() => setMode(m)}
                    style={[styles.modeCard, mode === m && (m === 'sea' ? styles.modeCardSeaActive : styles.modeCardAirActive)]}
                    activeOpacity={0.8}
                  >
                    <MaterialIcons
                      name={m === 'sea' ? 'directions-boat' : 'flight'}
                      size={36}
                      color={mode === m ? (m === 'sea' ? CYAN : PURPLE) : MUTED}
                    />
                    <Text style={[styles.modeName, mode === m && { color: m === 'sea' ? CYAN : PURPLE }]}>
                      {m === 'sea' ? 'Sea Cargo' : 'Air Cargo'}
                    </Text>
                    <Text style={styles.modeSub}>
                      {m === 'sea' ? '30–60 days · Economical' : '2–14 days · Fastest'}
                    </Text>
                  </TouchableOpacity>
                ))}
              </View>

              {/* AR Measure CTA */}
              {arAvailable && (
                <TouchableOpacity
                  style={[styles.arButton, arLoading && { opacity: 0.6 }]}
                  onPress={handleARMeasure}
                  disabled={arLoading}
                  activeOpacity={0.8}
                >
                  {arLoading
                    ? <ActivityIndicator size="small" color={CANVAS} />
                    : <MaterialIcons name="view-in-ar" size={20} color={CANVAS} />
                  }
                  <Text style={styles.arButtonText}>
                    {arLoading ? 'Opening AR Camera…' : measuredDims ? 'Re-Measure with AR' : 'Measure Box with AR Camera'}
                  </Text>
                </TouchableOpacity>
              )}

              {measuredDims && (
                <View style={styles.measuredCard}>
                  <MaterialIcons name="check-circle" size={16} color={GREEN} />
                  <Text style={styles.measuredText}>
                    Measured: {measuredDims.length} × {measuredDims.width} × {measuredDims.height} cm
                    {'  '}({Math.round(measuredDims.confidence * 100)}% confidence)
                  </Text>
                </View>
              )}

              {!arAvailable && (
                <View style={styles.infoCard}>
                  <MaterialIcons name="info-outline" size={14} color={AMBER} />
                  <Text style={styles.infoText}>
                    AR measurement not available on this device. You can manually enter dimensions or select a standard box size.
                  </Text>
                </View>
              )}
            </View>
          )}

          {/* ── Step 1: Origin ───────────────────────────────────────── */}
          {step === 1 && (
            <View style={styles.stepContent}>
              <Text style={styles.stepTitle}>{mode === 'sea' ? 'Where are you shipping from?' : 'Select your region'}</Text>
              <Text style={styles.stepSub}>
                {mode === 'sea' ? 'Pick the area closest to your location.' : 'Choose the zone that covers your country.'}
              </Text>

              <ScrollView
                style={styles.originList}
                nestedScrollEnabled
                showsVerticalScrollIndicator={false}
              >
                {origins.map(o => (
                  <TouchableOpacity
                    key={o}
                    style={[styles.originItem, originKey === o && styles.originItemSelected]}
                    onPress={() => setOriginKey(o)}
                    activeOpacity={0.7}
                  >
                    <Text style={[styles.originText, originKey === o && { color: CYAN }]} numberOfLines={2}>{o}</Text>
                    {originKey === o && <MaterialIcons name="check" size={16} color={CYAN} />}
                  </TouchableOpacity>
                ))}
              </ScrollView>
            </View>
          )}

          {/* ── Step 2: Box size ─────────────────────────────────────── */}
          {step === 2 && (
            <View style={styles.stepContent}>
              <Text style={styles.stepTitle}>Select your box size</Text>
              <Text style={styles.stepSub}>
                {measuredDims
                  ? `Your box was measured at ${measuredDims.length}×${measuredDims.width}×${measuredDims.height} cm. Best match highlighted.`
                  : 'Choose the standard size closest to your box.'}
              </Text>

              <View style={styles.boxGrid}>
                {BOX_SIZES.map(size => (
                  <BoxCard
                    key={size.id}
                    size={size}
                    selected={sizeId === size.id}
                    onPress={() => setSizeId(size.id)}
                  />
                ))}
              </View>

              {/* Quantity + weight */}
              <View style={styles.qtyRow}>
                <View style={{ flex: 1 }}>
                  <Text style={styles.inputLabel}>Number of boxes</Text>
                  <View style={styles.qtyControl}>
                    <TouchableOpacity style={styles.qtyBtn} onPress={() => setQty(q => Math.max(1, q - 1))}>
                      <Text style={styles.qtyBtnText}>−</Text>
                    </TouchableOpacity>
                    <Text style={styles.qtyValue}>{qty}</Text>
                    <TouchableOpacity style={styles.qtyBtn} onPress={() => setQty(q => Math.min(20, q + 1))}>
                      <Text style={styles.qtyBtnText}>+</Text>
                    </TouchableOpacity>
                  </View>
                </View>

                {mode === 'air' && (
                  <View style={{ flex: 1, marginLeft: 12 }}>
                    <Text style={styles.inputLabel}>Weight (kg)</Text>
                    <TextInput
                      style={styles.textInput}
                      value={String(weightKg)}
                      onChangeText={t => setWeightKg(Number(t) || 0)}
                      keyboardType="decimal-pad"
                      placeholderTextColor={MUTED}
                    />
                  </View>
                )}
              </View>

              {selectedSize && (
                <View style={styles.sizeSummary}>
                  <MaterialIcons name="straighten" size={14} color={CYAN} />
                  <Text style={styles.sizeSummaryText}>
                    {selectedSize.name} · {selectedSize.dimensions} ·{' '}
                    <Text style={{ color: CYAN }}>{cbm} m³</Text>
                    {qty > 1 && <Text style={{ color: AMBER }}> × {qty} = {(cbm * qty).toFixed(4)} m³</Text>}
                  </Text>
                </View>
              )}
            </View>
          )}

          {/* ── Step 3: Province ─────────────────────────────────────── */}
          {step === 3 && (
            <View style={styles.stepContent}>
              <Text style={styles.stepTitle}>Where in the Philippines?</Text>
              <Text style={styles.stepSub}>Enter the recipient's province or city for local delivery pricing.</Text>

              <View style={styles.inputRow}>
                <MaterialIcons name="place" size={18} color={MUTED} style={{ marginRight: 8 }} />
                <TextInput
                  style={[styles.textInput, { flex: 1 }]}
                  placeholder="e.g. Cebu, Davao, Metro Manila…"
                  placeholderTextColor={MUTED}
                  value={province}
                  onChangeText={setProvince}
                  autoCapitalize="words"
                  returnKeyType="done"
                />
              </View>

              {province.length > 0 && (
                <View style={[styles.zoneCard, { borderColor: resolvedZone ? 'rgba(0,255,136,0.2)' : 'rgba(255,171,0,0.2)' }]}>
                  <View style={[styles.zoneDot, { backgroundColor: resolvedZone ? GREEN : AMBER }]} />
                  <View style={{ flex: 1 }}>
                    {resolvedZone ? (
                      <>
                        <Text style={{ color: GREEN, fontSize: 11, fontWeight: '600' }}>Zone matched</Text>
                        <Text style={{ color: TEXT, fontSize: 13, marginTop: 2, fontWeight: '600' }}>
                          {resolvedZone.zone_code}
                        </Text>
                      </>
                    ) : (
                      <Text style={{ color: AMBER, fontSize: 11 }}>Province not found — PH delivery excluded from quote</Text>
                    )}
                  </View>
                </View>
              )}

              {!province && (
                <View style={styles.infoCard}>
                  <MaterialIcons name="info-outline" size={14} color={MUTED} />
                  <Text style={styles.infoText}>Optional — skip to get freight-only price.</Text>
                </View>
              )}
            </View>
          )}

          {/* ── Step 4: Result ───────────────────────────────────────── */}
          {step === 4 && result && (
            <View style={styles.stepContent}>
              <Text style={styles.stepTitle}>Your estimate</Text>
              <Text style={styles.stepSub}>
                Indicative price{result.transit_days ? ` · ${result.transit_days} transit` : ''}
              </Text>

              {result.lines.map((line, i) => (
                <QuoteLine key={i} {...line} />
              ))}

              {/* Total */}
              <LinearGradient
                colors={['rgba(0,229,255,0.10)', 'rgba(168,85,247,0.08)']}
                style={styles.totalCard}
                start={{ x: 0, y: 0 }} end={{ x: 1, y: 1 }}
              >
                <View style={styles.totalRow}>
                  <View>
                    <Text style={styles.totalLabel}>Estimated Total</Text>
                    <Text style={styles.totalSub}>{result.origin_currency} · incl. PH delivery</Text>
                  </View>
                  <Text style={styles.totalAmount}>
                    {fmtAmount(result.total_origin_currency, result.origin_currency)}
                  </Text>
                </View>
                {cbm > 0 && (
                  <View style={styles.cbmRow}>
                    <Text style={styles.cbmLabel}>CBM</Text>
                    <Text style={styles.cbmValue}>{cbm} m³</Text>
                  </View>
                )}
              </LinearGradient>

              {/* Actions */}
              <TouchableOpacity style={styles.bookBtn} onPress={handleBookNow} activeOpacity={0.85}>
                <LinearGradient
                  colors={[CYAN, PURPLE]}
                  style={styles.bookBtnGradient}
                  start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }}
                >
                  <MaterialIcons name="local-shipping" size={18} color={CANVAS} />
                  <Text style={styles.bookBtnText}>Book This Shipment</Text>
                  <MaterialIcons name="arrow-forward" size={16} color={CANVAS} />
                </LinearGradient>
              </TouchableOpacity>

              <TouchableOpacity
                style={styles.restartBtn}
                onPress={() => { setStep(0); setResult(null); }}
                activeOpacity={0.7}
              >
                <Text style={styles.restartText}>Start a new quote</Text>
              </TouchableOpacity>

              <Text style={styles.disclaimer}>
                Rates are indicative estimates. PH delivery converted from PHP using approximate FX rates. Final invoice at booking.
              </Text>
            </View>
          )}

        </Animated.View>
      </ScrollView>

      {/* Bottom nav (steps 0–3) */}
      {step < 4 && (
        <View style={[styles.footer, { paddingBottom: insets.bottom + 8 }]}>
          <TouchableOpacity onPress={handleBack} style={styles.footerBackBtn}>
            <MaterialIcons name="chevron-left" size={20} color={MUTED} />
            <Text style={styles.footerBackText}>{step === 0 ? 'Cancel' : 'Back'}</Text>
          </TouchableOpacity>

          <TouchableOpacity onPress={handleNext} activeOpacity={0.85}>
            <LinearGradient
              colors={[CYAN, PURPLE]}
              style={styles.footerNextBtn}
              start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }}
            >
              <Text style={styles.footerNextText}>{step === 3 ? 'Calculate Price' : 'Next'}</Text>
              <MaterialIcons name="chevron-right" size={18} color={CANVAS} />
            </LinearGradient>
          </TouchableOpacity>
        </View>
      )}
    </View>
  );
}

// ── Styles ─────────────────────────────────────────────────────────────────────

const styles = StyleSheet.create({
  container:    { flex: 1, backgroundColor: CANVAS },
  header:       { flexDirection: 'row', alignItems: 'center', paddingHorizontal: 16, paddingVertical: 12 },
  backBtn:      { marginRight: 12, padding: 4 },
  title:        { color: TEXT, fontSize: 18, fontWeight: '700' },
  subtitle:     { color: MUTED, fontSize: 11, marginTop: 1 },
  arBadge:      { flexDirection: 'row', alignItems: 'center', gap: 4, paddingHorizontal: 8, paddingVertical: 3, borderRadius: 8, borderWidth: 1, borderColor: 'rgba(0,229,255,0.25)', backgroundColor: 'rgba(0,229,255,0.06)' },
  arBadgeText:  { color: CYAN, fontSize: 10, fontWeight: '700' },

  stepRow:      { flexDirection: 'row', alignItems: 'center', paddingHorizontal: 16, paddingBottom: 16 },
  stepItem:     { alignItems: 'center', gap: 4 },
  stepDot:      { width: 20, height: 20, borderRadius: 10, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, alignItems: 'center', justifyContent: 'center' },
  stepDotActive:{ backgroundColor: CYAN, borderColor: CYAN },
  stepNum:      { color: MUTED, fontSize: 9, fontWeight: '700' },
  stepLabel:    { color: MUTED, fontSize: 9 },
  stepLine:     { flex: 1, height: 1, backgroundColor: BORDER, marginBottom: 16 },
  stepLineActive:{ backgroundColor: CYAN },

  scrollContent:{ paddingBottom: 24 },
  stepContent:  { paddingHorizontal: 16, gap: 16 },
  stepTitle:    { color: TEXT, fontSize: 20, fontWeight: '700', marginBottom: 2 },
  stepSub:      { color: MUTED, fontSize: 13, lineHeight: 18, marginBottom: 8 },

  modeRow:      { flexDirection: 'row', gap: 12 },
  modeCard:     { flex: 1, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 16, padding: 20, alignItems: 'center', gap: 8 },
  modeCardSeaActive:  { borderColor: 'rgba(0,229,255,0.4)', backgroundColor: 'rgba(0,229,255,0.06)' },
  modeCardAirActive:  { borderColor: 'rgba(168,85,247,0.4)', backgroundColor: 'rgba(168,85,247,0.06)' },
  modeName:     { color: TEXT, fontSize: 14, fontWeight: '700' },
  modeSub:      { color: MUTED, fontSize: 11, textAlign: 'center' },

  arButton:     { flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 10, paddingVertical: 14, borderRadius: 14, backgroundColor: CYAN },
  arButtonText: { color: CANVAS, fontSize: 14, fontWeight: '700' },
  measuredCard: { flexDirection: 'row', alignItems: 'center', gap: 8, padding: 12, borderRadius: 12, backgroundColor: 'rgba(0,255,136,0.06)', borderWidth: 1, borderColor: 'rgba(0,255,136,0.15)' },
  measuredText: { color: GREEN, fontSize: 12, flex: 1 },

  originList:   { maxHeight: 340, borderWidth: 1, borderColor: BORDER, borderRadius: 12, overflow: 'hidden' },
  originItem:   { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', paddingVertical: 12, paddingHorizontal: 14, borderBottomWidth: 1, borderColor: BORDER },
  originItemSelected: { backgroundColor: 'rgba(0,229,255,0.06)' },
  originText:   { color: TEXT, fontSize: 13, flex: 1, marginRight: 8 },

  boxGrid:      { flexDirection: 'row', flexWrap: 'wrap', gap: 10 },
  boxCard:      { width: (SCREEN_W - 32 - 10 * 2) / 3, padding: 10, borderRadius: 14, borderWidth: 1, borderColor: BORDER, backgroundColor: GLASS, alignItems: 'center', gap: 4 },
  boxCardSelected: { borderColor: 'rgba(0,229,255,0.5)', backgroundColor: 'rgba(0,229,255,0.06)' },
  boxIcon:      { width: 36, height: 28, position: 'relative', marginBottom: 4 },
  boxFace:      { position: 'absolute', bottom: 0, left: 0, borderWidth: 1, borderRadius: 2, backgroundColor: 'rgba(255,255,255,0.04)' },
  boxTop:       { position: 'absolute', top: 0, right: 0, width: 12, height: 10, borderWidth: 1, borderRadius: 1, backgroundColor: 'rgba(255,255,255,0.03)', transform: [{ skewX: '-20deg' }] },
  boxName:      { color: TEXT, fontSize: 11, fontWeight: '700' },
  boxDims:      { color: MUTED, fontSize: 9 },
  boxCbm:       { color: 'rgba(168,85,247,0.8)', fontSize: 9 },
  boxCheck:     { position: 'absolute', top: -5, right: -5, width: 16, height: 16, borderRadius: 8, backgroundColor: CYAN, alignItems: 'center', justifyContent: 'center' },

  qtyRow:       { flexDirection: 'row', alignItems: 'flex-start' },
  qtyControl:   { flexDirection: 'row', alignItems: 'center', borderWidth: 1, borderColor: BORDER, borderRadius: 10, overflow: 'hidden', alignSelf: 'flex-start', marginTop: 8 },
  qtyBtn:       { width: 36, height: 36, alignItems: 'center', justifyContent: 'center', backgroundColor: GLASS },
  qtyBtnText:   { color: TEXT, fontSize: 18, lineHeight: 20 },
  qtyValue:     { width: 36, textAlign: 'center', color: TEXT, fontSize: 16, fontWeight: '700' },

  sizeSummary:  { flexDirection: 'row', alignItems: 'center', gap: 8, padding: 12, borderRadius: 12, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER },
  sizeSummaryText: { color: MUTED, fontSize: 12, flex: 1 },

  inputRow:     { flexDirection: 'row', alignItems: 'center', borderWidth: 1, borderColor: BORDER, borderRadius: 12, paddingHorizontal: 12, paddingVertical: 8, backgroundColor: GLASS },
  inputLabel:   { color: MUTED, fontSize: 11, marginBottom: 6 },
  textInput:    { color: TEXT, fontSize: 14, paddingVertical: 8, paddingHorizontal: 0, borderWidth: 0, backgroundColor: 'transparent', flex: 1 },

  zoneCard:     { flexDirection: 'row', alignItems: 'center', gap: 10, padding: 14, borderRadius: 12, borderWidth: 1, backgroundColor: GLASS },
  zoneDot:      { width: 8, height: 8, borderRadius: 4 },

  quoteLine:    { flexDirection: 'row', alignItems: 'flex-start', padding: 14, borderRadius: 12, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, marginBottom: 8 },
  quoteLineLabel:{ fontSize: 13, fontWeight: '600' },
  quoteLineNote: { color: MUTED, fontSize: 11, marginTop: 2, lineHeight: 15 },
  quoteLineAmount:{ color: TEXT, fontSize: 13, fontWeight: '700', fontFamily: Platform.OS === 'ios' ? 'Menlo' : 'monospace' },

  totalCard:    { borderRadius: 16, padding: 16, borderWidth: 1, borderColor: 'rgba(0,229,255,0.2)', marginVertical: 8 },
  totalRow:     { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  totalLabel:   { color: MUTED, fontSize: 11 },
  totalSub:     { color: 'rgba(255,255,255,0.3)', fontSize: 10, marginTop: 2 },
  totalAmount:  { color: CYAN, fontSize: 26, fontWeight: '800' },
  cbmRow:       { flexDirection: 'row', justifyContent: 'space-between', marginTop: 10, paddingTop: 10, borderTopWidth: 1, borderColor: BORDER },
  cbmLabel:     { color: MUTED, fontSize: 11 },
  cbmValue:     { color: PURPLE, fontSize: 11, fontFamily: Platform.OS === 'ios' ? 'Menlo' : 'monospace' },

  bookBtn:      { borderRadius: 14, overflow: 'hidden', marginTop: 4 },
  bookBtnGradient:{ flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 8, paddingVertical: 16 },
  bookBtnText:  { color: CANVAS, fontSize: 15, fontWeight: '700' },
  restartBtn:   { padding: 14, alignItems: 'center' },
  restartText:  { color: MUTED, fontSize: 13 },
  disclaimer:   { color: 'rgba(255,255,255,0.2)', fontSize: 10, textAlign: 'center', lineHeight: 14 },

  footer:       { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', paddingHorizontal: 16, paddingTop: 12, borderTopWidth: 1, borderColor: BORDER, backgroundColor: CANVAS },
  footerBackBtn:{ flexDirection: 'row', alignItems: 'center', gap: 2 },
  footerBackText:{ color: MUTED, fontSize: 14 },
  footerNextBtn:{ flexDirection: 'row', alignItems: 'center', gap: 4, paddingHorizontal: 20, paddingVertical: 12, borderRadius: 12 },
  footerNextText:{ color: CANVAS, fontSize: 14, fontWeight: '700' },

  infoCard:     { flexDirection: 'row', alignItems: 'flex-start', gap: 8, padding: 12, borderRadius: 12, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER },
  infoText:     { color: MUTED, fontSize: 12, flex: 1, lineHeight: 16 },
});

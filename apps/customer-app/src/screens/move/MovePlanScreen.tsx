/**
 * Move app — The plan. Confirm the route and the load, add hands, see the
 * price broken down, lock it in.
 *
 * The route and items come pre-filled from the prompt and stay editable: the
 * app reads the sentence, the customer confirms it, the server prices it.
 * Every row under "How this is priced" is a row the server returned, and a
 * total is only shown when those rows add up to it.
 */
import React, { useEffect, useMemo, useRef, useState } from 'react';
import {
  ActivityIndicator, Alert, KeyboardAvoidingView, Platform, Pressable, ScrollView,
  StyleSheet, Text, TextInput, View,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useDispatch, useSelector } from 'react-redux';
import { Ionicons } from '@expo/vector-icons';
import type { AppDispatch, RootState } from '../../store';
import { shipmentsActions } from '../../store';
import * as shipmentsService from '../../services/api/shipments';
import { getMyTenant } from '../../services/api/tenant';
import {
  accessorialLabel, COUNTRY_FOR_CURRENCY, linesReconcile, listAccessorials, quoteLines, quoteMove, quoteTotal,
  type AccessorialCatalog, type MoveQuote,
} from '../../services/api/move';
import type { ParsedItem, ParsedMove } from './parsePrompt';
import { formatKg, formatMoney } from './format';
import { HEADING, M } from './theme';
import { Ambient, Field, Label, Panel, PrimaryButton, Stepper, Toggle, TopBar } from './ui';

function makeKey(): string {
  return `mv_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 10)}`;
}

function apiMessage(err: any): string {
  return err?.data?.error?.message ?? err?.data?.message ?? err?.message ?? 'Something went wrong.';
}

const positive = (v: string) => v === '' || (Number.isFinite(Number(v)) && Number(v) > 0);
const cm = (v: string) => (v ? Math.round(Number(v)) : undefined);

export function MovePlanScreen({ navigation, route }: { navigation: any; route: any }) {
  const insets = useSafeAreaInsets();
  const dispatch = useDispatch<AppDispatch>();
  const auth = useSelector((s: RootState) => s.auth);
  const parsed: ParsedMove = route.params?.parsed ?? { items: [], from: '', to: '', when: '' };

  const [items, setItems] = useState<ParsedItem[]>(parsed.items.length ? parsed.items : [{ name: '', qty: 1 }]);
  const [fromLine, setFromLine] = useState(parsed.from);
  const [fromCity, setFromCity] = useState('');
  const [toLine, setToLine] = useState(parsed.to);
  const [toCity, setToCity] = useState('');
  const [country, setCountry] = useState('PH');
  const [weightKg, setWeightKg] = useState('');
  const [lengthCm, setLengthCm] = useState('');
  const [widthCm, setWidthCm] = useState('');
  const [heightCm, setHeightCm] = useState('');
  const [catalog, setCatalog] = useState<AccessorialCatalog | null | undefined>(undefined);
  const [units, setUnits] = useState<Record<string, number>>({});
  const [threshold, setThreshold] = useState(true);
  const [quote, setQuote] = useState<MoveQuote | null>(null);
  const [quoteError, setQuoteError] = useState<string | null>(null);
  const [quoting, setQuoting] = useState(false);
  const [booking, setBooking] = useState(false);
  const quoteSeq = useRef(0);

  useEffect(() => {
    getMyTenant()
      .then((t) => {
        const c = t.currency ? COUNTRY_FOR_CURRENCY[t.currency] : undefined;
        if (c) setCountry(c);
      })
      .catch(() => {});
    listAccessorials().then(setCatalog).catch(() => setCatalog(null));
  }, []);

  const grams = Math.round((parseFloat(weightKg) || 0) * 1000);
  const ready =
    fromLine.trim().length >= 5 && fromCity.trim().length >= 2 &&
    toLine.trim().length >= 5 && toCity.trim().length >= 2 &&
    country.trim().length === 2 && grams > 0 &&
    positive(lengthCm) && positive(widthCm) && positive(heightCm);

  const accessorials = useMemo(
    () => Object.entries(units).filter(([, n]) => n > 0).map(([code, n]) => ({ code, units: n })),
    [units],
  );

  const address = (line1: string, city: string): shipmentsService.AddressInput => ({
    line1: line1.trim(),
    city: city.trim(),
    province: city.trim(),
    postal_code: '0000',
    country_code: country.trim().toUpperCase(),
  });

  // Re-price when anything priced changes. The latest request wins.
  useEffect(() => {
    if (!ready) {
      setQuote(null);
      setQuoteError(null);
      return;
    }
    const seq = ++quoteSeq.current;
    const timer = setTimeout(async () => {
      setQuoting(true);
      try {
        const q = await quoteMove({
          service_type: 'standard',
          weight_grams: grams,
          length_cm: cm(lengthCm),
          width_cm: cm(widthCm),
          height_cm: cm(heightCm),
          origin: address(fromLine, fromCity),
          destination: address(toLine, toCity),
          accessorials,
        });
        if (seq === quoteSeq.current) {
          setQuote(q);
          setQuoteError(null);
        }
      } catch (err) {
        if (seq === quoteSeq.current) {
          setQuote(null);
          setQuoteError(apiMessage(err));
        }
      } finally {
        if (seq === quoteSeq.current) setQuoting(false);
      }
    }, 600);
    return () => clearTimeout(timer);
    // address() reads the fields listed here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, grams, lengthCm, widthCm, heightCm, fromLine, fromCity, toLine, toCity, country, accessorials]);

  const total = quote ? quoteTotal(quote) : null;
  const reconciles = quote ? linesReconcile(quote) : false;

  async function lockItIn() {
    if (!quote || !total || booking) return;
    setBooking(true);
    const description =
      items.filter((i) => i.name.trim()).map((i) => (i.qty > 1 ? `${i.qty} × ${i.name.trim()}` : i.name.trim())).join(', ') || 'Move';
    const notes = [
      parsed.when ? `Requested time: ${parsed.when}` : null,
      threshold ? 'Threshold delivery: place items inside the door' : null,
      ...accessorials.map((a) => `${accessorialLabel(a.code)}${a.units > 1 ? ` × ${a.units}` : ''}`),
    ].filter(Boolean).join(' · ');

    const request: shipmentsService.CreateShipmentRequest = {
      customer_name: auth.name ?? 'Customer',
      customer_phone: auth.phone ?? '',
      customer_email: auth.email ?? undefined,
      origin: address(fromLine, fromCity),
      destination: address(toLine, toCity),
      service_type: 'standard',
      weight_grams: grams,
      length_cm: cm(lengthCm),
      width_cm: cm(widthCm),
      height_cm: cm(heightCm),
      description,
      special_instructions: notes || undefined,
      idempotency_key: makeKey(),
    };

    try {
      let res: shipmentsService.ShipmentResponse;
      try {
        // The quote token pays online at the quoted price.
        res = await shipmentsService.createShipment({ ...request, quote_token: quote.quote_token });
      } catch (err) {
        // A tenant without online payment refuses any quote token outright.
        // Book without it; the server prices the job itself.
        if (!/quote_token/i.test(apiMessage(err))) throw err;
        res = await shipmentsService.createShipment({ ...request, idempotency_key: makeKey() });
      }

      const awb = res.awb ?? res.tracking_number;
      dispatch(shipmentsActions.addShipment({
        id: res.id,
        awb,
        type: 'local',
        status: 'confirmed',
        origin: `${fromLine.trim()}, ${fromCity.trim()}`,
        destination: `${toLine.trim()}, ${toCity.trim()}`,
        description,
        weight: String(grams / 1000),
        bookedAt: new Date().toLocaleString(),
        recipientName: auth.name ?? undefined,
        recipientPhone: auth.phone ?? undefined,
      }));

      if (res.checkout_url) {
        navigation.navigate('PaymentWebView', { checkoutUrl: res.checkout_url, shipmentId: res.id });
      } else {
        navigation.replace('MoveBooked', {
          awb,
          id: res.id,
          quotedText: formatMoney(total.cents, total.currency, { whole: true }),
          when: parsed.when,
        });
      }
    } catch (err) {
      Alert.alert("Couldn't book", apiMessage(err));
    } finally {
      setBooking(false);
    }
  }

  const services = catalog?.items ?? [];

  return (
    <KeyboardAvoidingView style={s.root} behavior={Platform.OS === 'ios' ? 'padding' : undefined}>
      <Ambient />
      <TopBar label="The plan" onBack={() => navigation.goBack()} />

      <ScrollView
        contentContainerStyle={{ paddingHorizontal: 20, paddingTop: 6, paddingBottom: 240 + insets.bottom }}
        keyboardShouldPersistTaps="handled"
      >
        <Panel>
          {quote ? (
            <>
              <View style={s.vehicleRow}>
                <Text style={s.vehicle}>{quote.vehicle_label || 'Your vehicle'}</Text>
                {quote.distance_km != null && <Text style={s.vehicleMeta}>{quote.distance_km.toFixed(1)} km</Text>}
              </View>
              {quote.billable_grams != null && (
                <Text style={s.note}>
                  Billable {formatKg(quote.billable_grams)}{quote.billable_basis ? ` · ${quote.billable_basis}` : ''}
                </Text>
              )}
            </>
          ) : quoting ? (
            <View style={s.vehicleRow}>
              <Text style={s.vehicle}>Pricing…</Text>
              <ActivityIndicator color={M.accent} />
            </View>
          ) : quoteError ? (
            <>
              <Text style={s.vehicle}>No price yet</Text>
              <Text style={[s.note, { color: M.amberText }]}>{quoteError}</Text>
            </>
          ) : (
            <>
              <Text style={s.vehicle}>Confirm the move</Text>
              <Text style={s.note}>Check the route and the load below. The vehicle and the price follow.</Text>
            </>
          )}
        </Panel>

        <Label>From your message</Label>
        <Panel tone="accent">
          <Field label="Pickup address" value={fromLine} onChangeText={setFromLine} placeholder="Street and number" />
          <Field label="Pickup city" value={fromCity} onChangeText={setFromCity} placeholder="City" style={s.gap} />
          <View style={s.divider} />
          <Field label="Drop-off address" value={toLine} onChangeText={setToLine} placeholder="Street and number" />
          <Field label="Drop-off city" value={toCity} onChangeText={setToCity} placeholder="City" style={s.gap} />
          <View style={[s.rowGap, s.gap]}>
            <Field
              label="Country"
              value={country}
              onChangeText={(v) => setCountry(v.toUpperCase().slice(0, 2))}
              autoCapitalize="characters"
              maxLength={2}
              style={{ flex: 0, width: 96 }}
            />
            {!!parsed.when && (
              <View style={{ flex: 1, justifyContent: 'flex-end' }}>
                <Text style={s.fieldNote}>You asked for {parsed.when}. The driver confirms the pickup time.</Text>
              </View>
            )}
          </View>
        </Panel>

        <Label
          right={
            <Pressable onPress={() => setItems((l) => [...l, { name: '', qty: 1 }])} hitSlop={10} accessibilityRole="button">
              <Text style={s.link}>Add item</Text>
            </Pressable>
          }
        >
          Manifest
        </Label>
        <View style={{ gap: 10 }}>
          {items.map((item, i) => (
            <View key={i} style={s.itemRow}>
              <View style={s.itemIcon}><Ionicons name="cube-outline" size={20} color={M.accent} /></View>
              <TextInput
                value={item.name}
                onChangeText={(name) => setItems((l) => l.map((x, j) => (j === i ? { ...x, name } : x)))}
                placeholder="Item"
                placeholderTextColor={M.faint}
                style={s.itemName}
              />
              <Stepper
                value={item.qty}
                min={1}
                label={item.name || 'items'}
                onChange={(qty) => setItems((l) => l.map((x, j) => (j === i ? { ...x, qty } : x)))}
              />
              {items.length > 1 && (
                <Pressable
                  onPress={() => setItems((l) => l.filter((_, j) => j !== i))}
                  hitSlop={8}
                  accessibilityRole="button"
                  accessibilityLabel={`Remove ${item.name || 'item'}`}
                >
                  <Ionicons name="close" size={18} color={M.faint} />
                </Pressable>
              )}
            </View>
          ))}
        </View>

        <Label>The load</Label>
        <Panel>
          <Field label="Total weight (kg)" value={weightKg} onChangeText={setWeightKg} keyboardType="decimal-pad" placeholder="e.g. 187" />
          <View style={[s.rowGap, s.gap]}>
            <Field label="Length cm" value={lengthCm} onChangeText={setLengthCm} keyboardType="number-pad" placeholder="L" />
            <Field label="Width cm" value={widthCm} onChangeText={setWidthCm} keyboardType="number-pad" placeholder="W" />
            <Field label="Height cm" value={heightCm} onChangeText={setHeightCm} keyboardType="number-pad" placeholder="H" />
          </View>
          <Text style={[s.note, { marginTop: 10 }]}>
            Charged on the greater of scale weight and volumetric weight, at the standard 5000 cm³/kg factor.
          </Text>
        </Panel>

        <Label>How this is priced</Label>
        <Panel>
          {quote ? (
            <>
              <View style={{ gap: 12 }}>
                {quoteLines(quote).map((line, i) => (
                  <View key={`${line.label}-${i}`} style={s.lineRow}>
                    <View style={{ flex: 1 }}>
                      <Text style={s.lineLabel}>{line.label}</Text>
                      {!!line.note && <Text style={s.lineNote}>{line.note}</Text>}
                    </View>
                    <Text style={s.lineAmount}>{formatMoney(line.amount_cents, total?.currency)}</Text>
                  </View>
                ))}
              </View>
              <View style={s.divider} />
              <View style={s.lineRow}>
                <Text style={s.lineMuted}>Billable weight</Text>
                <Text style={s.lineMutedValue}>
                  {quote.billable_grams != null ? formatKg(quote.billable_grams) : '—'}
                  {quote.billable_basis ? ` · ${quote.billable_basis}` : ''}
                </Text>
              </View>
              {!reconciles && (
                <Text style={[s.note, { color: M.amberText, marginTop: 10 }]}>
                  These rows don't add up to the price, so no price is shown. Change anything to re-price.
                </Text>
              )}
            </>
          ) : (
            <Text style={s.note}>The server prices the move once the route and the load are filled in.</Text>
          )}
        </Panel>

        <Label>Hands and muscle</Label>
        <View style={{ gap: 10 }}>
          {services.map((svc) => {
            const count = units[svc.code] ?? 0;
            const perFlight = svc.basis === 'stair_flight';
            const price = formatMoney(svc.amount_cents, catalog?.currency, { whole: true });
            return (
              <View key={svc.code} style={s.serviceRow}>
                <View style={{ flex: 1 }}>
                  <View style={s.serviceTop}>
                    <Text style={s.serviceLabel}>{accessorialLabel(svc.code)}</Text>
                    <Text style={s.servicePrice}>{perFlight ? `${price} per flight` : price}</Text>
                  </View>
                  {perFlight && <Text style={s.lineNote}>Stair flights at pickup and drop-off</Text>}
                </View>
                {perFlight ? (
                  <Stepper
                    value={count}
                    max={svc.max_units}
                    label="flights"
                    onChange={(n) => setUnits((u) => ({ ...u, [svc.code]: n }))}
                  />
                ) : (
                  <Toggle
                    value={count > 0}
                    label={accessorialLabel(svc.code)}
                    onChange={(on) => setUnits((u) => ({ ...u, [svc.code]: on ? 1 : 0 }))}
                  />
                )}
              </View>
            );
          })}
          <View style={s.serviceRow}>
            <View style={{ flex: 1 }}>
              <View style={s.serviceTop}>
                <Text style={s.serviceLabel}>Threshold delivery</Text>
                <Text style={[s.servicePrice, { color: M.accent }]}>Free</Text>
              </View>
              <Text style={s.lineNote}>Items go inside the door, not left outside</Text>
            </View>
            <Toggle value={threshold} onChange={setThreshold} label="Threshold delivery" />
          </View>
          {catalog === null && <Text style={s.note}>Paid add-ons aren't offered in your area yet.</Text>}
        </View>
      </ScrollView>

      <View style={[s.sheet, { bottom: insets.bottom + 14 }]}>
        <View style={s.sheetTop}>
          <View>
            <Text style={s.sheetLabel}>{quote ? 'All-in · held 15 min' : 'All-in'}</Text>
            <Text style={s.total}>
              {quote && reconciles && total ? formatMoney(total.cents, total.currency, { whole: true }) : '—'}
            </Text>
          </View>
          <Text style={s.sheetNote}>No surge · no fuel surcharge</Text>
        </View>
        <PrimaryButton
          label="LOCK IT IN"
          onPress={lockItIn}
          disabled={!quote || !reconciles || quoting}
          loading={booking}
          style={{ marginTop: 14 }}
        />
      </View>
    </KeyboardAvoidingView>
  );
}

const s = StyleSheet.create({
  root:           { flex: 1, backgroundColor: M.ground },
  vehicleRow:     { flexDirection: 'row', alignItems: 'baseline', justifyContent: 'space-between', gap: 10 },
  vehicle:        { fontFamily: HEADING, fontWeight: '700', fontSize: 19, color: M.ink },
  vehicleMeta:    { fontSize: 12, color: M.accent, fontVariant: ['tabular-nums'] },
  note:           { fontSize: 12, lineHeight: 18, color: M.faint, marginTop: 8 },
  fieldNote:      { fontSize: 11, lineHeight: 16, color: M.faint },
  gap:            { marginTop: 10 },
  rowGap:         { flexDirection: 'row', gap: 8 },
  divider:        { height: 1, backgroundColor: M.hairline, marginVertical: 14 },
  link:           { fontSize: 12, color: M.accent },
  itemRow:        { flexDirection: 'row', alignItems: 'center', gap: 12, padding: 12, borderRadius: 18, backgroundColor: 'rgba(255,255,255,0.035)', borderWidth: 1, borderColor: M.hairline },
  itemIcon:       { width: 40, height: 40, borderRadius: 12, backgroundColor: M.accentTint, alignItems: 'center', justifyContent: 'center' },
  itemName:       { flex: 1, minHeight: 44, fontFamily: HEADING, fontWeight: '700', fontSize: 17, color: M.ink, padding: 0 },
  lineRow:        { flexDirection: 'row', alignItems: 'baseline', justifyContent: 'space-between', gap: 12 },
  lineLabel:      { fontSize: 14, color: M.ink },
  lineNote:       { fontSize: 11, lineHeight: 16, color: M.faint, marginTop: 2 },
  lineAmount:     { fontFamily: HEADING, fontWeight: '700', fontSize: 17, color: M.ink, fontVariant: ['tabular-nums'] },
  lineMuted:      { fontSize: 13, color: M.muted },
  lineMutedValue: { fontSize: 13, color: M.ink, fontVariant: ['tabular-nums'] },
  serviceRow:     { flexDirection: 'row', alignItems: 'center', gap: 14, padding: 15, borderRadius: 18, backgroundColor: 'rgba(255,255,255,0.035)', borderWidth: 1, borderColor: M.hairline },
  serviceTop:     { flexDirection: 'row', alignItems: 'baseline', gap: 9, flexWrap: 'wrap' },
  serviceLabel:   { fontSize: 15, color: M.ink },
  servicePrice:   { fontSize: 12, color: M.muted, fontVariant: ['tabular-nums'] },
  sheet:          { position: 'absolute', left: 14, right: 14, borderRadius: 26, padding: 16, backgroundColor: M.sheet, borderWidth: 1, borderColor: 'rgba(255,255,255,0.12)' },
  sheetTop:       { flexDirection: 'row', alignItems: 'flex-end', justifyContent: 'space-between', gap: 12 },
  sheetLabel:     { fontSize: 10, letterSpacing: 2, textTransform: 'uppercase', color: M.label },
  total:          { fontFamily: HEADING, fontWeight: '700', fontSize: 42, lineHeight: 46, letterSpacing: -0.4, color: M.ink, fontVariant: ['tabular-nums'] },
  sheetNote:      { fontSize: 11, lineHeight: 16, color: M.faint, textAlign: 'right', flexShrink: 1 },
});

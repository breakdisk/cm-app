/**
 * Move app — Cancel. What cancelling now costs, from the server's preview
 * (`GET /v1/shipments/:id/cancellation-preview`, PR #162), and the confirm.
 *
 * The fee and refund are the server's numbers or none. Against a backend
 * without the preview, the screen falls back to the plain confirm.
 */
import React, { useEffect, useState } from 'react';
import { Alert, ScrollView, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useDispatch, useSelector } from 'react-redux';
import type { AppDispatch, RootState } from '../../store';
import { shipmentsActions } from '../../store';
import { cancelShipment, getCancellationPreview, type CancellationPreview } from '../../services/api/shipments';
import { describeCancellation } from '../../utils/cancellation';
import { formatMoney } from './format';
import { HEADING_LIGHT, M } from './theme';
import { Ambient, GhostButton, Label, Panel, PrimaryButton, TopBar } from './ui';

const TIER_LABEL: Record<CancellationPreview['tier'], string> = {
  early: 'More than 48 hours out',
  late: 'Inside the fee window',
  same_day: 'At or after pickup time',
  unscheduled: 'No pickup time booked',
};

const POLICY = [
  { title: 'Well ahead of pickup', body: 'Cancelling costs nothing.' },
  { title: 'Close to pickup', body: 'A late-cancellation fee may apply. The exact amount shows here before you confirm.' },
  { title: 'At or after the pickup time', body: 'A move-day fee may apply, because the driver is already committed.' },
  { title: 'Refunds', body: 'Anything refunded goes back the way you paid.' },
];

export function MoveCancelScreen({ navigation, route }: { navigation: any; route: any }) {
  const insets = useSafeAreaInsets();
  const dispatch = useDispatch<AppDispatch>();
  const { id, awb } = route.params ?? {};
  const booked = useSelector((s: RootState) => s.shipments.list.find((x) => x.awb === awb));
  const [preview, setPreview] = useState<CancellationPreview | null | undefined>(undefined);
  const [cancelling, setCancelling] = useState(false);

  useEffect(() => {
    let alive = true;
    getCancellationPreview(id)
      .catch(() => null)
      .then((p) => { if (alive) setPreview(p); });
    return () => { alive = false; };
  }, [id]);

  const terms = describeCancellation(preview ?? null);
  const charged = !!preview && preview.policy_applies && preview.fee_bps > 0;
  const accent = charged ? M.penalty : M.accent;
  const feeText = !preview
    ? '—'
    : !charged
      ? 'Free'
      : preview.fee_cents != null
        ? formatMoney(preview.fee_cents, preview.currency, { whole: true })
        : `${+(preview.fee_bps / 100).toFixed(2)}%`;

  async function confirm() {
    setCancelling(true);
    try {
      await cancelShipment(id, 'Cancelled by customer in the Move app');
      if (booked) dispatch(shipmentsActions.updateShipment({ ...booked, status: 'cancelled' }));
      navigation.replace('MoveCancelled', {
        awb,
        refundText: preview?.refund_cents != null ? formatMoney(preview.refund_cents, preview.currency, { whole: true }) : null,
      });
    } catch (err: any) {
      const message = err?.status === 403
        ? "This account can't cancel moves yet. Contact support to cancel this one."
        : err?.message ?? "Couldn't cancel. Try again.";
      Alert.alert('Not cancelled', message);
    } finally {
      setCancelling(false);
    }
  }

  return (
    <View style={s.root}>
      <Ambient />
      <TopBar label={`Cancel ${awb ?? ''}`} onBack={() => navigation.goBack()} />
      <ScrollView contentContainerStyle={{ paddingHorizontal: 20, paddingTop: 8, paddingBottom: insets.bottom + 150 }}>
        <Panel tone={charged ? 'penalty' : 'accent'}>
          <Text style={[s.kicker, { color: accent }]}>{preview ? TIER_LABEL[preview.tier] : preview === undefined ? 'Checking…' : 'Cancellation'}</Text>
          <Text style={[s.fee, { color: accent }]}>{feeText}</Text>
          <Text style={s.rule}>{[terms.headline, terms.detail].filter(Boolean).join(' ')}</Text>
        </Panel>

        {preview?.refund_cents != null && (
          <View style={s.refund}>
            <Text style={s.refundLabel}>Refunded to you</Text>
            <Text style={s.refundValue}>{formatMoney(preview.refund_cents, preview.currency, { whole: true })}</Text>
          </View>
        )}

        <Label>The rest of the policy</Label>
        <View style={{ gap: 12 }}>
          {POLICY.map((p) => (
            <View key={p.title} style={s.policyRow}>
              <View style={s.policyDot} />
              <View style={{ flex: 1 }}>
                <Text style={s.policyTitle}>{p.title}</Text>
                <Text style={s.policyBody}>{p.body}</Text>
              </View>
            </View>
          ))}
        </View>
      </ScrollView>

      <View style={[s.actions, { bottom: insets.bottom + 20 }]}>
        <PrimaryButton
          danger
          label={terms.canCancel ? 'CANCEL THIS MOVE' : "CAN'T BE CANCELLED"}
          onPress={confirm}
          disabled={!terms.canCancel || preview === undefined}
          loading={cancelling}
        />
        <GhostButton label="Keep my booking" onPress={() => navigation.goBack()} />
      </View>
    </View>
  );
}

const s = StyleSheet.create({
  root:        { flex: 1, backgroundColor: M.ground },
  kicker:      { fontSize: 10, letterSpacing: 2, textTransform: 'uppercase' },
  fee:         { fontFamily: HEADING_LIGHT, fontWeight: '300', fontSize: 40, lineHeight: 44, marginTop: 10, fontVariant: ['tabular-nums'] },
  rule:        { fontSize: 13, lineHeight: 20, color: M.muted, marginTop: 10 },
  refund:      { flexDirection: 'row', alignItems: 'baseline', justifyContent: 'space-between', marginTop: 16, padding: 15, borderRadius: 16, backgroundColor: M.accentTint, borderWidth: 1, borderColor: M.hairline },
  refundLabel: { fontSize: 12, letterSpacing: 1.8, textTransform: 'uppercase', color: M.muted },
  refundValue: { fontSize: 20, color: M.accent, fontVariant: ['tabular-nums'] },
  policyRow:   { flexDirection: 'row', gap: 12, alignItems: 'flex-start' },
  policyDot:   { width: 6, height: 6, borderRadius: 3, backgroundColor: 'rgba(242,246,250,0.3)', marginTop: 6 },
  policyTitle: { fontSize: 13, lineHeight: 18, color: M.ink },
  policyBody:  { fontSize: 11, lineHeight: 17, color: M.faint, marginTop: 3 },
  actions:     { position: 'absolute', left: 14, right: 14, gap: 9 },
});

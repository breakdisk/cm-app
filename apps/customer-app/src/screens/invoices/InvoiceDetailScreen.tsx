/**
 * Customer App — Payment Receipt Detail screen
 * Accessible from InvoicesScreen or via deep-link:
 *   logisticos://invoices/:id
 */
import React, { useCallback, useEffect, useState } from 'react';
import {
  View, Text, StyleSheet, ScrollView, Pressable, ActivityIndicator, Alert,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Ionicons } from '@expo/vector-icons';
import { useNavigation, useRoute } from '@react-navigation/native';
import { useDispatch, useSelector } from 'react-redux';
import type { RootState, AppDispatch } from '../../store';
import { setLoading, setDetail, setError } from '../../store/slices/invoices';
import { resendInvoice } from '../../services/api/invoices';
import { getInvoice } from '../../services/api/invoices';

const CANVAS  = '#050810';
const CYAN    = '#00E5FF';
const GREEN   = '#00FF88';
const PURPLE  = '#A855F7';
const GLASS   = 'rgba(255,255,255,0.04)';
const BORDER  = 'rgba(255,255,255,0.08)';

function formatAmount(cents: number, currency = 'PHP'): string {
  return `${currency} ${(cents / 100).toFixed(2)}`;
}

function fmt(iso: string | null | undefined): string {
  if (!iso) return '—';
  try {
    return new Date(iso).toLocaleString('en-PH', {
      year: 'numeric', month: 'short', day: 'numeric',
      hour: '2-digit', minute: '2-digit',
    });
  } catch {
    return iso;
  }
}

function chargeLabel(type: string): string {
  const map: Record<string, string> = {
    base_freight:           'Base Freight',
    fuel_surcharge:         'Fuel Surcharge',
    insurance_fee:          'Shipment Insurance',
    weight_surcharge:       'Weight Surcharge',
    dimensional_surcharge:  'Dimensional Surcharge',
    cod_handling_fee:       'COD Handling',
    manual_adjustment:      'Adjustment',
  };
  return map[type] ?? type.replace(/_/g, ' ').replace(/\b\w/g, c => c.toUpperCase());
}

export function InvoiceDetailScreen() {
  const insets     = useSafeAreaInsets();
  const navigation = useNavigation<any>();
  const route      = useRoute<any>();
  const dispatch   = useDispatch<AppDispatch>();

  const invoiceId = route.params?.invoiceId as string;
  const detail    = useSelector((s: RootState) => s.invoices.byId[invoiceId]);
  const loading   = useSelector((s: RootState) => s.invoices.loading);
  const error     = useSelector((s: RootState) => s.invoices.error);
  const [resending, setResending] = useState(false);

  const load = useCallback(async () => {
    if (!invoiceId) return;
    dispatch(setLoading(true));
    try {
      const data = await getInvoice(invoiceId);
      dispatch(setDetail(data));
    } catch (e: any) {
      dispatch(setError(e?.message ?? 'Failed to load receipt'));
    }
  }, [invoiceId, dispatch]);

  const handleResend = useCallback(async () => {
    if (!invoiceId) return;
    setResending(true);
    try {
      await resendInvoice(invoiceId);
      Alert.alert('Receipt Sent', 'A copy of your receipt has been sent to your email address.');
    } catch (e: any) {
      Alert.alert('Error', e?.message ?? 'Failed to send receipt. Please try again.');
    } finally {
      setResending(false);
    }
  }, [invoiceId]);

  useEffect(() => {
    if (!detail) load();
  }, [detail, load]);

  if (loading && !detail) {
    return (
      <View style={[s.container, { paddingTop: insets.top }, s.centered]}>
        <ActivityIndicator color={CYAN} size="large" />
      </View>
    );
  }

  if (error && !detail) {
    return (
      <View style={[s.container, { paddingTop: insets.top }, s.centered]}>
        <Ionicons name="alert-circle-outline" size={32} color="#FF3B5C" />
        <Text style={s.errorText}>{error}</Text>
        <Pressable onPress={load} style={s.retryBtn}>
          <Text style={s.retryText}>Try Again</Text>
        </Pressable>
      </View>
    );
  }

  if (!detail) return null;

  const subtotal  = detail.line_items.reduce((sum, li) => {
    const gross = li.unit_price.amount * li.quantity;
    const disc  = li.discount?.amount ?? 0;
    return sum + gross - disc;
  }, 0);
  const total = detail.total_due?.amount ?? 0;
  const vat   = total - subtotal;
  const currency = detail.line_items[0]?.unit_price.currency ?? 'PHP';

  return (
    <View style={[s.container, { paddingTop: insets.top }]}>
      {/* Header */}
      <View style={s.header}>
        <Pressable onPress={() => navigation.goBack()} style={s.backBtn}>
          <Ionicons name="arrow-back" size={20} color={CYAN} />
        </Pressable>
        <Text style={s.headerTitle}>Receipt</Text>
        <View style={{ width: 36 }} />
      </View>

      <ScrollView contentContainerStyle={{ padding: 16, paddingBottom: 40 + insets.bottom }}>

        {/* Receipt number + status hero */}
        <View style={s.hero}>
          <View style={[s.receiptIconBig, { backgroundColor: CYAN + '18' }]}>
            <Ionicons name="receipt-outline" size={28} color={CYAN} />
          </View>
          <Text style={s.receiptNum}>{detail.invoice_number}</Text>
          <Text style={[s.status, { color: detail.status === 'paid' ? GREEN : CYAN }]}>
            {detail.status.toUpperCase()}
          </Text>
          <Text style={s.totalHero}>{formatAmount(total, currency)}</Text>
          {detail.paid_at && (
            <Text style={s.paidAt}>Paid · {fmt(detail.paid_at)}</Text>
          )}
        </View>

        {/* Timestamps */}
        <View style={s.metaCard}>
          {[
            { label: 'Issued',  value: fmt(detail.issued_at) },
            { label: 'Due',     value: fmt(detail.due_at) },
            { label: 'Paid',    value: fmt(detail.paid_at) },
            { label: 'Type',    value: detail.invoice_type.replace(/_/g, ' ').toUpperCase() },
          ].map(row => (
            <View key={row.label} style={s.metaRow}>
              <Text style={s.metaLabel}>{row.label}</Text>
              <Text style={s.metaValue}>{row.value}</Text>
            </View>
          ))}
        </View>

        {/* Line items */}
        <Text style={s.sectionTitle}>Charges</Text>
        <View style={s.lineItemsCard}>
          {detail.line_items.map((li, idx) => {
            const gross = li.unit_price.amount * li.quantity;
            const disc  = li.discount?.amount ?? 0;
            const net   = gross - disc;
            return (
              <View key={idx} style={[s.lineRow, idx > 0 && s.lineRowBorder]}>
                <View style={{ flex: 1 }}>
                  <Text style={s.chargeType}>{chargeLabel(li.charge_type)}</Text>
                  <Text style={s.chargeDesc}>{li.description}</Text>
                  {li.quantity > 1 && (
                    <Text style={s.chargeQty}>
                      {li.quantity} × {formatAmount(li.unit_price.amount, currency)}
                    </Text>
                  )}
                </View>
                <Text style={s.chargeAmt}>{formatAmount(net, currency)}</Text>
              </View>
            );
          })}
        </View>

        {/* Totals */}
        <View style={s.totalsCard}>
          <View style={s.totalRow}>
            <Text style={s.totalLabel}>Subtotal</Text>
            <Text style={s.totalValue}>{formatAmount(subtotal, currency)}</Text>
          </View>
          <View style={s.totalRow}>
            <Text style={s.totalLabel}>VAT (12%)</Text>
            <Text style={s.totalValue}>{formatAmount(vat, currency)}</Text>
          </View>
          <View style={[s.totalRow, s.grandRow]}>
            <Text style={s.grandLabel}>Total Paid</Text>
            <Text style={[s.grandValue, { color: GREEN }]}>{formatAmount(total, currency)}</Text>
          </View>
        </View>

        {/* Receipt ID — scannable for support */}
        <View style={s.idCard}>
          <Ionicons name="qr-code-outline" size={14} color={PURPLE} />
          <Text style={s.idLabel}>Receipt ID</Text>
          <Text style={s.idValue} selectable>{detail.id}</Text>
        </View>

        {/* Resend receipt to email */}
        <Pressable
          onPress={handleResend}
          disabled={resending}
          style={({ pressed }) => [s.resendBtn, { opacity: pressed || resending ? 0.7 : 1 }]}
        >
          {resending ? (
            <ActivityIndicator size="small" color={CYAN} />
          ) : (
            <Ionicons name="mail-outline" size={16} color={CYAN} />
          )}
          <Text style={s.resendText}>
            {resending ? 'Sending...' : 'Email This Receipt'}
          </Text>
        </Pressable>

      </ScrollView>
    </View>
  );
}

const s = StyleSheet.create({
  container:       { flex: 1, backgroundColor: CANVAS },
  centered:        { alignItems: 'center', justifyContent: 'center', flex: 1, gap: 12 },
  errorText:       { fontSize: 13, color: 'rgba(255,255,255,0.5)', textAlign: 'center', paddingHorizontal: 32 },
  retryBtn:        { backgroundColor: CYAN + '20', borderWidth: 1, borderColor: CYAN + '40', borderRadius: 10, paddingHorizontal: 20, paddingVertical: 10 },
  retryText:       { color: CYAN, fontFamily: 'SpaceGrotesk-SemiBold', fontSize: 13 },

  header:          { flexDirection: 'row', alignItems: 'center', paddingHorizontal: 16, paddingVertical: 12, borderBottomWidth: 1, borderBottomColor: BORDER },
  backBtn:         { width: 36, height: 36, borderRadius: 10, backgroundColor: GLASS, alignItems: 'center', justifyContent: 'center' },
  headerTitle:     { flex: 1, textAlign: 'center', fontSize: 16, fontWeight: '700', color: '#FFF', fontFamily: 'SpaceGrotesk-Bold' },

  hero:            { alignItems: 'center', paddingVertical: 28, gap: 8 },
  receiptIconBig:  { width: 60, height: 60, borderRadius: 18, alignItems: 'center', justifyContent: 'center', marginBottom: 4 },
  receiptNum:      { fontSize: 14, fontFamily: 'JetBrainsMono-Regular', color: 'rgba(255,255,255,0.6)' },
  status:          { fontSize: 10, fontFamily: 'JetBrainsMono-Regular', letterSpacing: 1.5 },
  totalHero:       { fontSize: 30, fontWeight: '700', color: '#FFF', fontFamily: 'SpaceGrotesk-Bold', marginTop: 4 },
  paidAt:          { fontSize: 11, color: 'rgba(0,255,136,0.6)', fontFamily: 'JetBrainsMono-Regular' },

  metaCard:        { backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 14, padding: 16, marginBottom: 16, gap: 10 },
  metaRow:         { flexDirection: 'row', justifyContent: 'space-between' },
  metaLabel:       { fontSize: 10, color: 'rgba(255,255,255,0.3)', fontFamily: 'JetBrainsMono-Regular', textTransform: 'uppercase', letterSpacing: 0.8 },
  metaValue:       { fontSize: 12, color: '#FFF', fontFamily: 'JetBrainsMono-Regular' },

  sectionTitle:    { fontSize: 10, color: 'rgba(255,255,255,0.3)', fontFamily: 'JetBrainsMono-Regular', textTransform: 'uppercase', letterSpacing: 1.5, marginBottom: 8 },

  lineItemsCard:   { backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 14, marginBottom: 12, overflow: 'hidden' },
  lineRow:         { flexDirection: 'row', alignItems: 'flex-start', gap: 12, padding: 14 },
  lineRowBorder:   { borderTopWidth: 1, borderTopColor: BORDER },
  chargeType:      { fontSize: 13, color: '#FFF', fontFamily: 'SpaceGrotesk-SemiBold' },
  chargeDesc:      { fontSize: 10, color: 'rgba(255,255,255,0.35)', fontFamily: 'JetBrainsMono-Regular', marginTop: 2 },
  chargeQty:       { fontSize: 10, color: 'rgba(255,255,255,0.3)', fontFamily: 'JetBrainsMono-Regular', marginTop: 2 },
  chargeAmt:       { fontSize: 13, color: '#FFF', fontFamily: 'SpaceGrotesk-SemiBold' },

  totalsCard:      { backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 14, padding: 16, marginBottom: 12, gap: 10 },
  totalRow:        { flexDirection: 'row', justifyContent: 'space-between' },
  totalLabel:      { fontSize: 12, color: 'rgba(255,255,255,0.4)', fontFamily: 'JetBrainsMono-Regular' },
  totalValue:      { fontSize: 12, color: 'rgba(255,255,255,0.7)', fontFamily: 'JetBrainsMono-Regular' },
  grandRow:        { borderTopWidth: 1, borderTopColor: BORDER, paddingTop: 10, marginTop: 2 },
  grandLabel:      { fontSize: 14, fontWeight: '700', color: '#FFF', fontFamily: 'SpaceGrotesk-Bold' },
  grandValue:      { fontSize: 16, fontWeight: '700', fontFamily: 'SpaceGrotesk-Bold' },

  idCard:          { flexDirection: 'row', alignItems: 'center', gap: 8, backgroundColor: PURPLE + '10', borderWidth: 1, borderColor: PURPLE + '30', borderRadius: 10, padding: 12 },
  idLabel:         { fontSize: 10, color: PURPLE, fontFamily: 'JetBrainsMono-Regular', textTransform: 'uppercase', letterSpacing: 0.5 },
  idValue:         { flex: 1, fontSize: 10, color: 'rgba(255,255,255,0.4)', fontFamily: 'JetBrainsMono-Regular' },

  resendBtn:       { flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 8, paddingVertical: 14, paddingHorizontal: 20, backgroundColor: CYAN + '10', borderWidth: 1, borderColor: CYAN + '30', borderRadius: 14, marginTop: 4 },
  resendText:      { fontSize: 14, fontWeight: '600', color: CYAN, fontFamily: 'SpaceGrotesk-SemiBold' },
});

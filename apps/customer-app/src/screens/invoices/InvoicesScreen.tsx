/**
 * Customer App — Payment Receipts list screen
 * Accessible from Profile → Payment Receipts.
 * Deep-link: logisticos://invoices  (list)
 */
import React, { useCallback, useEffect } from 'react';
import {
  View, Text, StyleSheet, FlatList, Pressable,
  ActivityIndicator, RefreshControl,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Ionicons } from '@expo/vector-icons';
import { useNavigation } from '@react-navigation/native';
import { useDispatch, useSelector } from 'react-redux';
import type { RootState, AppDispatch } from '../../store';
import { setLoading, setList, setError } from '../../store/slices/invoices';
import { listCustomerInvoices } from '../../services/api/invoices';
import type { InvoiceSummary } from '../../services/api/invoices';

const CANVAS = '#050810';
const CYAN   = '#00E5FF';
const GREEN  = '#00FF88';
const GLASS  = 'rgba(255,255,255,0.04)';
const BORDER = 'rgba(255,255,255,0.08)';

function formatAmount(cents: number, currency = 'PHP'): string {
  return `${currency} ${(cents / 100).toFixed(2)}`;
}

function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleDateString('en-PH', {
      year: 'numeric', month: 'short', day: 'numeric',
    });
  } catch {
    return iso;
  }
}

function statusColor(status: string): string {
  switch (status) {
    case 'paid':      return GREEN;
    case 'issued':    return CYAN;
    case 'overdue':   return '#FF3B5C';
    case 'cancelled': return 'rgba(255,255,255,0.3)';
    default:          return 'rgba(255,255,255,0.4)';
  }
}

function ReceiptCard({ item, onPress }: { item: InvoiceSummary; onPress: () => void }) {
  const color = statusColor(item.status);
  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [s.card, { opacity: pressed ? 0.75 : 1 }]}
    >
      {/* Receipt icon + number */}
      <View style={s.cardLeft}>
        <View style={[s.iconBox, { backgroundColor: CYAN + '18' }]}>
          <Ionicons name="receipt-outline" size={18} color={CYAN} />
        </View>
        <View style={{ flex: 1 }}>
          <Text style={s.receiptNum} numberOfLines={1}>{item.invoice_number}</Text>
          <Text style={s.receiptDate}>{formatDate(item.issued_at)}</Text>
        </View>
      </View>

      {/* Amount + status */}
      <View style={s.cardRight}>
        <Text style={[s.amount, { color: GREEN }]}>{formatAmount(item.total_cents)}</Text>
        <View style={[s.statusPill, { borderColor: color + '50', backgroundColor: color + '14' }]}>
          <Text style={[s.statusText, { color }]}>{item.status.toUpperCase()}</Text>
        </View>
      </View>

      <Ionicons name="chevron-forward" size={14} color="rgba(255,255,255,0.2)" />
    </Pressable>
  );
}

export function InvoicesScreen() {
  const insets     = useSafeAreaInsets();
  const navigation = useNavigation<any>();
  const dispatch   = useDispatch<AppDispatch>();

  const customerId = useSelector((s: RootState) => s.auth.customerId);
  const { list, loading, error } = useSelector((s: RootState) => s.invoices);

  const load = useCallback(async () => {
    if (!customerId) return;
    dispatch(setLoading(true));
    try {
      const data = await listCustomerInvoices(customerId);
      dispatch(setList(data));
    } catch (e: any) {
      dispatch(setError(e?.message ?? 'Failed to load receipts'));
    }
  }, [customerId, dispatch]);

  useEffect(() => { load(); }, [load]);

  return (
    <View style={[s.container, { paddingTop: insets.top }]}>
      {/* Header */}
      <View style={s.header}>
        <Pressable onPress={() => navigation.goBack()} style={s.backBtn}>
          <Ionicons name="arrow-back" size={20} color={CYAN} />
        </Pressable>
        <Text style={s.headerTitle}>Payment Receipts</Text>
        <View style={{ width: 36 }} />
      </View>

      {loading && list.length === 0 ? (
        <View style={s.centered}>
          <ActivityIndicator color={CYAN} size="large" />
        </View>
      ) : error ? (
        <View style={s.centered}>
          <Ionicons name="alert-circle-outline" size={32} color="#FF3B5C" />
          <Text style={s.errorText}>{error}</Text>
          <Pressable onPress={load} style={s.retryBtn}>
            <Text style={s.retryText}>Try Again</Text>
          </Pressable>
        </View>
      ) : (
        <FlatList
          data={list}
          keyExtractor={i => i.invoice_id}
          contentContainerStyle={{ padding: 16, paddingBottom: 32 + insets.bottom }}
          refreshControl={
            <RefreshControl
              refreshing={loading}
              onRefresh={load}
              tintColor={CYAN}
              colors={[CYAN]}
            />
          }
          ListHeaderComponent={
            list.length > 0 ? (
              <Text style={s.listHeader}>{list.length} receipt{list.length !== 1 ? 's' : ''}</Text>
            ) : null
          }
          ListEmptyComponent={
            <View style={s.empty}>
              <Ionicons name="receipt-outline" size={40} color="rgba(255,255,255,0.1)" />
              <Text style={s.emptyTitle}>No Receipts Yet</Text>
              <Text style={s.emptySubtitle}>
                Payment receipts will appear here after your deliveries are completed.
              </Text>
            </View>
          }
          renderItem={({ item }) => (
            <ReceiptCard
              item={item}
              onPress={() => navigation.navigate('InvoiceDetail', { invoiceId: item.invoice_id })}
            />
          )}
        />
      )}
    </View>
  );
}

const s = StyleSheet.create({
  container:    { flex: 1, backgroundColor: CANVAS },
  header:       { flexDirection: 'row', alignItems: 'center', paddingHorizontal: 16, paddingVertical: 12, borderBottomWidth: 1, borderBottomColor: BORDER },
  backBtn:      { width: 36, height: 36, borderRadius: 10, backgroundColor: GLASS, alignItems: 'center', justifyContent: 'center' },
  headerTitle:  { flex: 1, textAlign: 'center', fontSize: 16, fontWeight: '700', color: '#FFF', fontFamily: 'SpaceGrotesk-Bold' },

  centered:     { flex: 1, alignItems: 'center', justifyContent: 'center', gap: 12 },
  errorText:    { fontSize: 13, color: 'rgba(255,255,255,0.5)', textAlign: 'center', paddingHorizontal: 32 },
  retryBtn:     { backgroundColor: CYAN + '20', borderWidth: 1, borderColor: CYAN + '40', borderRadius: 10, paddingHorizontal: 20, paddingVertical: 10 },
  retryText:    { color: CYAN, fontFamily: 'SpaceGrotesk-SemiBold', fontSize: 13 },

  listHeader:   { fontSize: 10, color: 'rgba(255,255,255,0.3)', fontFamily: 'JetBrainsMono-Regular', textTransform: 'uppercase', letterSpacing: 1.2, marginBottom: 12 },

  card:         { flexDirection: 'row', alignItems: 'center', backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 14, padding: 14, marginBottom: 10, gap: 12 },
  cardLeft:     { flex: 1, flexDirection: 'row', alignItems: 'center', gap: 12 },
  iconBox:      { width: 36, height: 36, borderRadius: 10, alignItems: 'center', justifyContent: 'center' },
  receiptNum:   { fontSize: 13, fontFamily: 'JetBrainsMono-Regular', color: '#FFF' },
  receiptDate:  { fontSize: 10, color: 'rgba(255,255,255,0.35)', fontFamily: 'JetBrainsMono-Regular', marginTop: 2 },

  cardRight:    { alignItems: 'flex-end', gap: 6 },
  amount:       { fontSize: 14, fontFamily: 'SpaceGrotesk-Bold', fontWeight: '700' },
  statusPill:   { borderWidth: 1, borderRadius: 6, paddingHorizontal: 6, paddingVertical: 2 },
  statusText:   { fontSize: 8, fontFamily: 'JetBrainsMono-Regular', letterSpacing: 0.5 },

  empty:        { alignItems: 'center', paddingTop: 80, gap: 12 },
  emptyTitle:   { fontSize: 16, fontWeight: '700', color: 'rgba(255,255,255,0.4)', fontFamily: 'SpaceGrotesk-Bold' },
  emptySubtitle:{ fontSize: 13, color: 'rgba(255,255,255,0.25)', textAlign: 'center', paddingHorizontal: 32, lineHeight: 20 },
});

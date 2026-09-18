/**
 * Move app — Payments. What this has cost, and what is owed. One screen:
 * invoices and per-job charges are one dataset (the handoff: do not split them).
 *
 * "Paid this month" is derived from the same list the cards render — never a
 * standalone figure.
 */
import React, { useEffect, useMemo, useState } from 'react';
import { ActivityIndicator, Alert, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useSelector } from 'react-redux';
import type { RootState } from '../../store';
import {
  getInvoice, listCustomerInvoices, resendInvoice, type InvoiceDetail, type InvoiceSummary,
} from '../../services/api/invoices';
import { formatMoney } from './format';
import { HEADING, M } from './theme';
import { Ambient, Panel, TopBar } from './ui';
import { CreditPanel } from './RewardsPanels';

type Filter = 'all' | 'unpaid' | 'paid';

const isPaid = (i: InvoiceSummary) => i.status === 'paid' || !!i.paid_at;
const isDue = (i: InvoiceSummary) => !isPaid(i) && !['draft', 'void', 'cancelled'].includes(i.status);
/** Summaries carry major units (`total_php`); formatting takes minor units. */
const minor = (major: number) => Math.round(major * 100);

function day(iso: string | null | undefined): string {
  if (!iso) return '';
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? '' : d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}

export function MovePaymentsScreen({ navigation }: { navigation: any }) {
  const insets = useSafeAreaInsets();
  const customerId = useSelector((s: RootState) => s.auth.customerId);
  const [invoices, setInvoices] = useState<InvoiceSummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>('all');
  const [open, setOpen] = useState<string | null>(null);
  const [details, setDetails] = useState<Record<string, InvoiceDetail | 'loading' | 'error'>>({});

  useEffect(() => {
    if (!customerId) {
      setInvoices([]);
      return;
    }
    listCustomerInvoices(customerId)
      .then(setInvoices)
      .catch((err: any) => {
        setError(err?.message ?? "Couldn't load payments.");
        setInvoices([]);
      });
  }, [customerId]);

  const paidThisMonth = useMemo(() => {
    const now = new Date();
    return (invoices ?? [])
      .filter((i) => {
        if (!isPaid(i)) return false;
        const d = new Date(i.paid_at ?? i.created_at);
        return d.getFullYear() === now.getFullYear() && d.getMonth() === now.getMonth();
      })
      .reduce((n, i) => n + minor(i.total_php), 0);
  }, [invoices]);

  const due = (invoices ?? []).find(isDue);
  const shown = (invoices ?? []).filter((i) => (filter === 'all' ? true : filter === 'paid' ? isPaid(i) : !isPaid(i)));

  function toggle(inv: InvoiceSummary) {
    const next = open === inv.id ? null : inv.id;
    setOpen(next);
    if (next && !details[inv.id]) {
      setDetails((d) => ({ ...d, [inv.id]: 'loading' }));
      getInvoice(inv.id)
        .then((detail) => setDetails((d) => ({ ...d, [inv.id]: detail })))
        .catch(() => setDetails((d) => ({ ...d, [inv.id]: 'error' })));
    }
  }

  async function resend(inv: InvoiceSummary) {
    try {
      await resendInvoice(inv.id);
      Alert.alert('Receipt sent', 'We emailed it to the address on your account.');
    } catch (err: any) {
      Alert.alert("Couldn't send", err?.message ?? 'Try again later.');
    }
  }

  return (
    <View style={s.root}>
      <Ambient />
      <TopBar label="Payments" onBack={() => navigation.goBack()} />
      <ScrollView contentContainerStyle={{ paddingHorizontal: 20, paddingTop: 8, paddingBottom: insets.bottom + 44 }}>
        <Text style={s.kicker}>Paid this month</Text>
        <Text style={s.hero}>{invoices === null ? '—' : formatMoney(paidThisMonth, 'PHP', { whole: true })}</Text>
        <Text style={s.note}>
          {invoices === null ? 'Loading…' : `${(invoices ?? []).length} invoice${(invoices ?? []).length === 1 ? '' : 's'} on your account`}
        </Text>
        {!!error && <Text style={[s.note, { color: M.amberText }]}>{error}</Text>}

        <View style={s.filters}>
          {(['all', 'unpaid', 'paid'] as Filter[]).map((f) => (
            <Pressable
              key={f}
              onPress={() => setFilter(f)}
              accessibilityRole="button"
              accessibilityState={{ selected: filter === f }}
              style={[s.filter, filter === f && s.filterOn]}
            >
              <Text style={[s.filterText, filter === f && { color: M.accent }]}>{f[0].toUpperCase() + f.slice(1)}</Text>
            </Pressable>
          ))}
        </View>

        {!!due && filter !== 'paid' && (
          <Panel tone="amber" style={{ marginTop: 20 }}>
            <Text style={[s.kicker, { color: M.amber }]}>{due.due_date ? `Due ${day(due.due_date)}` : 'Unpaid'}</Text>
            <Text style={s.dueAmount}>{formatMoney(minor(due.total_php), 'PHP', { whole: true })}</Text>
            <Text style={s.dueMeta}>{due.invoice_number} · {due.awb_count} AWB{due.awb_count === 1 ? '' : 's'}</Text>
          </Panel>
        )}

        <CreditPanel />

        {invoices === null && <ActivityIndicator color={M.accent} style={{ marginTop: 30 }} />}
        {invoices !== null && shown.length === 0 && <Text style={[s.note, { marginTop: 20 }]}>Nothing here yet.</Text>}

        <View style={{ marginTop: 14, gap: 10 }}>
          {shown.map((inv) => {
            const paid = isPaid(inv);
            const detail = details[inv.id];
            const isOpen = open === inv.id;
            return (
              <View key={inv.id} style={[s.card, !paid && { borderColor: M.amberBorder }]}>
                <Pressable onPress={() => toggle(inv)} accessibilityRole="button" accessibilityState={{ expanded: isOpen }} style={{ padding: 16 }}>
                  <View style={s.cardTop}>
                    <Text style={s.number}>{inv.invoice_number}</Text>
                    <Text style={[s.badge, paid ? s.badgePaid : s.badgeUnpaid]}>{paid ? 'Paid' : 'Unpaid'}</Text>
                  </View>
                  <View style={s.cardMid}>
                    <Text style={s.period}>{day(inv.period_from || inv.created_at)} · {inv.awb_count} AWB{inv.awb_count === 1 ? '' : 's'}</Text>
                    <Text style={s.total}>{formatMoney(minor(inv.total_php), 'PHP', { whole: true })}</Text>
                  </View>
                  <View style={s.divider} />
                  <View style={s.cardMid}>
                    <Text style={s.vat}>{formatMoney(minor(inv.vat_php), 'PHP')} VAT included</Text>
                    <Text style={s.toggle}>{isOpen ? 'Hide jobs' : 'Show jobs'}</Text>
                  </View>
                </Pressable>
                {isOpen && (
                  <View style={{ paddingHorizontal: 16, paddingBottom: 16 }}>
                    <Text style={s.kicker}>Jobs on this invoice</Text>
                    {detail === 'loading' && <ActivityIndicator color={M.accent} style={{ marginVertical: 12 }} />}
                    {detail === 'error' && <Text style={s.note}>Couldn't load the lines.</Text>}
                    {typeof detail === 'object' && detail.line_items.map((ln, i) => (
                      <View key={i} style={s.line}>
                        <View style={{ flex: 1 }}>
                          <Text style={s.lineLabel}>{ln.description}</Text>
                          <Text style={s.lineMeta}>
                            {ln.quantity > 1 ? `${ln.quantity} × ` : ''}{formatMoney(ln.unit_price.amount, ln.unit_price.currency ?? detail.currency)}
                          </Text>
                        </View>
                      </View>
                    ))}
                    <Pressable onPress={() => resend(inv)} accessibilityRole="button" style={({ pressed }) => [s.resend, pressed && { transform: [{ scale: 0.96 }] }]}>
                      <Text style={s.resendText}>Resend receipt</Text>
                    </Pressable>
                  </View>
                )}
              </View>
            );
          })}
        </View>
      </ScrollView>
    </View>
  );
}

const s = StyleSheet.create({
  root:        { flex: 1, backgroundColor: M.ground },
  kicker:      { fontSize: 10, letterSpacing: 2, textTransform: 'uppercase', color: M.label },
  hero:        { fontFamily: HEADING, fontWeight: '700', fontSize: 58, lineHeight: 62, letterSpacing: -1, color: M.ink, marginTop: 4, fontVariant: ['tabular-nums'] },
  note:        { marginTop: 10, fontSize: 13, color: M.muted },
  filters:     { flexDirection: 'row', gap: 8, marginTop: 22 },
  filter:      { height: 44, paddingHorizontal: 16, borderRadius: 13, justifyContent: 'center', backgroundColor: 'rgba(255,255,255,0.04)', borderWidth: 1, borderColor: M.hairline },
  filterOn:    { backgroundColor: M.accentTint, borderColor: M.accentBorder },
  filterText:  { fontSize: 13, fontWeight: '600', color: M.muted },
  dueAmount:   { fontFamily: HEADING, fontWeight: '700', fontSize: 34, color: M.ink, marginTop: 10, fontVariant: ['tabular-nums'] },
  dueMeta:     { fontSize: 13, color: 'rgba(242,246,250,0.66)', marginTop: 4 },
  card:        { borderRadius: 18, backgroundColor: 'rgba(255,255,255,0.04)', borderWidth: 1, borderColor: M.hairline, overflow: 'hidden' },
  cardTop:     { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 10 },
  cardMid:     { flexDirection: 'row', alignItems: 'baseline', justifyContent: 'space-between', gap: 10, marginTop: 10 },
  number:      { fontFamily: HEADING, fontWeight: '700', fontSize: 17, color: M.ink },
  badge:       { fontSize: 11, fontWeight: '700', letterSpacing: 1, textTransform: 'uppercase', paddingHorizontal: 10, paddingVertical: 5, borderRadius: 8, borderWidth: 1, overflow: 'hidden' },
  badgePaid:   { color: M.accent, borderColor: M.accentBorder, backgroundColor: M.accentTint },
  badgeUnpaid: { color: M.amber, borderColor: M.amberBorder, backgroundColor: M.amberTint },
  period:      { fontSize: 13, color: 'rgba(242,246,250,0.64)' },
  total:       { fontFamily: HEADING, fontWeight: '700', fontSize: 22, color: M.ink, fontVariant: ['tabular-nums'] },
  divider:     { height: 1, backgroundColor: 'rgba(255,255,255,0.07)', marginTop: 12 },
  vat:         { fontSize: 12, color: M.muted },
  toggle:      { fontSize: 12, fontWeight: '600', color: M.accent },
  line:        { flexDirection: 'row', paddingVertical: 12, borderTopWidth: 1, borderTopColor: 'rgba(255,255,255,0.06)' },
  lineLabel:   { fontSize: 14, color: M.ink },
  lineMeta:    { fontSize: 11, color: 'rgba(242,246,250,0.55)', marginTop: 2, fontVariant: ['tabular-nums'] },
  resend:      { height: 46, marginTop: 10, borderRadius: 13, alignItems: 'center', justifyContent: 'center', backgroundColor: 'rgba(255,255,255,0.05)', borderWidth: 1, borderColor: M.hairlineStrong },
  resendText:  { fontSize: 13, fontWeight: '600', color: M.accent },
});

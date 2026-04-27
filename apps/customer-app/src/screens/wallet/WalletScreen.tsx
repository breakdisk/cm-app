import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  View, Text, StyleSheet, FlatList,
  Pressable, TextInput, ActivityIndicator, Animated,
} from 'react-native';
import { LinearGradient } from 'expo-linear-gradient';
import { Ionicons } from '@expo/vector-icons';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useNavigation } from '@react-navigation/native';
import { paymentsApi, type WalletData, type WalletTransaction } from '../../services/api/payments';

const CANVAS = '#050810';
const GREEN  = '#00FF88';
const CYAN   = '#00E5FF';
const RED    = '#FF3B5C';
const GLASS  = 'rgba(255,255,255,0.04)';
const BORDER = 'rgba(255,255,255,0.08)';

function fmtPhp(cents: number): string {
  return `₱${(cents / 100).toLocaleString('en-PH', { minimumFractionDigits: 0, maximumFractionDigits: 0 })}`;
}

// ── Withdraw bottom sheet ─────────────────────────────────────────────────────

function WithdrawSheet({
  wallet,
  visible,
  onClose,
  onSuccess,
}: {
  wallet: WalletData;
  visible: boolean;
  onClose: () => void;
  onSuccess: (updated: WalletData) => void;
}) {
  const [amount, setAmount] = useState('');
  const [saving, setSaving] = useState(false);
  const [error,  setError]  = useState<string | null>(null);
  const slideY = useRef(new Animated.Value(400)).current;

  useEffect(() => {
    if (visible) {
      setAmount('');
      setError(null);
      Animated.spring(slideY, { toValue: 0, useNativeDriver: true, tension: 80, friction: 10 }).start();
    } else {
      Animated.timing(slideY, { toValue: 400, duration: 200, useNativeDriver: true }).start();
    }
  }, [visible, slideY]);

  async function handleConfirm() {
    const parsed = parseFloat(amount);
    if (Number.isNaN(parsed) || parsed <= 0) {
      setError('Enter a valid amount');
      return;
    }
    const cents = Math.round(parsed * 100);
    if (cents > wallet.available_cents) {
      setError(`Exceeds available ${fmtPhp(wallet.available_cents)}`);
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const res = await paymentsApi.withdraw(cents);
      onSuccess(res.data.data);
    } catch (e: unknown) {
      const err = e as { message?: string };
      setError(err?.message ?? 'Withdrawal failed');
    } finally {
      setSaving(false);
    }
  }

  if (!visible) return null;

  return (
    <View style={ws.overlay}>
      <Pressable style={ws.backdrop} onPress={onClose} />
      <Animated.View style={[ws.sheet, { transform: [{ translateY: slideY }] }] as any}>
        <View style={ws.handle} />
        <Text style={ws.title}>Request Withdrawal</Text>
        <Text style={ws.avail}>Available: {fmtPhp(wallet.available_cents)}</Text>
        <TextInput
          style={ws.input}
          placeholder="Amount in ₱"
          placeholderTextColor="rgba(255,255,255,0.2)"
          keyboardType="decimal-pad"
          value={amount}
          onChangeText={setAmount}
        />
        {error ? <Text style={ws.error}>{error}</Text> : null}
        <View style={ws.btnRow}>
          <Pressable onPress={onClose} style={[ws.btn, ws.btnCancel]}>
            <Text style={ws.btnCancelText}>Cancel</Text>
          </Pressable>
          <Pressable
            onPress={() => { void handleConfirm(); }}
            disabled={saving}
            style={[ws.btn, ws.btnConfirm, saving && { opacity: 0.5 }]}
          >
            <Text style={ws.btnConfirmText}>{saving ? 'Submitting…' : 'Confirm'}</Text>
          </Pressable>
        </View>
      </Animated.View>
    </View>
  );
}

const ws = StyleSheet.create({
  overlay:       { position: 'absolute', top: 0, left: 0, right: 0, bottom: 0, justifyContent: 'flex-end', zIndex: 100 },
  backdrop:      { position: 'absolute', top: 0, left: 0, right: 0, bottom: 0, backgroundColor: 'rgba(0,0,0,0.7)' },
  sheet:         { backgroundColor: '#0A0E1A', borderTopLeftRadius: 24, borderTopRightRadius: 24, borderWidth: 1, borderColor: BORDER, padding: 24, paddingBottom: 40 },
  handle:        { width: 36, height: 4, backgroundColor: BORDER, borderRadius: 2, alignSelf: 'center', marginBottom: 20 },
  title:         { fontSize: 17, fontWeight: '700', color: '#FFF', fontFamily: 'SpaceGrotesk-Bold', marginBottom: 4 },
  avail:         { fontSize: 11, fontFamily: 'JetBrainsMono-Regular', color: 'rgba(255,255,255,0.35)', marginBottom: 16 },
  input:         { backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 10, paddingHorizontal: 14, paddingVertical: 12, fontSize: 16, color: '#FFF', fontFamily: 'JetBrainsMono-Regular', marginBottom: 8 },
  error:         { fontSize: 11, color: RED, fontFamily: 'JetBrainsMono-Regular', marginBottom: 8 },
  btnRow:        { flexDirection: 'row', gap: 10, marginTop: 8 },
  btn:           { flex: 1, borderRadius: 10, paddingVertical: 13, alignItems: 'center' },
  btnCancel:     { backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER },
  btnCancelText: { fontSize: 14, color: 'rgba(255,255,255,0.6)', fontFamily: 'SpaceGrotesk-SemiBold' },
  btnConfirm:    { backgroundColor: 'rgba(0,255,136,0.1)', borderWidth: 1, borderColor: 'rgba(0,255,136,0.3)' },
  btnConfirmText:{ fontSize: 14, color: GREEN, fontFamily: 'SpaceGrotesk-SemiBold' },
});

// ── Transaction row ───────────────────────────────────────────────────────────

function TxRow({ tx }: { tx: WalletTransaction }) {
  return (
    <View style={s.txRow}>
      <View style={[s.txIcon, { backgroundColor: (tx.type === 'credit' ? GREEN : RED) + '18' }]}>
        <Ionicons
          name={tx.type === 'credit' ? 'arrow-down-circle-outline' : 'arrow-up-circle-outline'}
          size={18}
          color={tx.type === 'credit' ? GREEN : RED}
        />
      </View>
      <View style={{ flex: 1 }}>
        <Text style={s.txDesc} numberOfLines={1}>{tx.description}</Text>
        <Text style={s.txDate}>{new Date(tx.created_at).toLocaleDateString()}</Text>
      </View>
      <Text style={[s.txAmount, { color: tx.type === 'credit' ? GREEN : RED }]}>
        {tx.type === 'credit' ? '+' : '-'}{fmtPhp(tx.amount_cents)}
      </Text>
    </View>
  );
}

// ── Main screen ───────────────────────────────────────────────────────────────

export function WalletScreen() {
  const insets     = useSafeAreaInsets();
  const navigation = useNavigation<any>();

  const [wallet,       setWallet]       = useState<WalletData | null>(null);
  const [transactions, setTransactions] = useState<WalletTransaction[]>([]);
  const [loading,      setLoading]      = useState(true);
  const [error,        setError]        = useState<string | null>(null);
  const [showWithdraw, setShowWithdraw] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [wRes, txRes] = await Promise.all([
        paymentsApi.getWallet(),
        paymentsApi.getTransactions(),
      ]);
      setWallet(wRes.data.data);
      setTransactions(txRes.data.data ?? []);
    } catch (e: unknown) {
      const err = e as { message?: string };
      setError(err?.message ?? 'Failed to load wallet');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { void load(); }, [load]);

  return (
    <View style={{ flex: 1, backgroundColor: CANVAS }}>
      {/* Header */}
      <View style={[s.header, { paddingTop: insets.top + 12 }]}>
        <Pressable onPress={() => navigation.goBack()} style={s.backBtn}>
          <Ionicons name="arrow-back" size={20} color="rgba(255,255,255,0.6)" />
        </Pressable>
        <Text style={s.headerTitle}>Wallet</Text>
        <Pressable onPress={() => { void load(); }} style={s.backBtn}>
          <Ionicons name="refresh-outline" size={18} color="rgba(255,255,255,0.4)" />
        </Pressable>
      </View>

      {loading ? (
        <View style={s.center}>
          <ActivityIndicator color={CYAN} />
        </View>
      ) : error ? (
        <View style={s.center}>
          <Text style={s.errorText}>{error}</Text>
          <Pressable onPress={() => { void load(); }} style={s.retryBtn}>
            <Text style={s.retryText}>Retry</Text>
          </Pressable>
        </View>
      ) : (
        <FlatList
          data={transactions}
          keyExtractor={(tx) => tx.id}
          contentContainerStyle={{ paddingBottom: insets.bottom + 24 }}
          ListHeaderComponent={
            <>
              {/* Balance card */}
              <LinearGradient
                colors={['rgba(0,255,136,0.12)', 'transparent']}
                style={s.balanceCard}
              >
                <Text style={s.balLabel}>WALLET BALANCE</Text>
                <Text style={s.balAmount}>{fmtPhp(wallet?.balance_cents ?? 0)}</Text>
                {wallet && wallet.reserved_cents > 0 && (
                  <Text style={s.balReserved}>
                    {fmtPhp(wallet.reserved_cents)} reserved · {fmtPhp(wallet.available_cents)} available
                  </Text>
                )}
                <Pressable
                  onPress={() => setShowWithdraw(true)}
                  disabled={!wallet || wallet.available_cents <= 0}
                  style={[s.withdrawBtn, (!wallet || wallet.available_cents <= 0) && { opacity: 0.4 }]}
                >
                  <Ionicons name="arrow-up-circle-outline" size={16} color={GREEN} />
                  <Text style={s.withdrawText}>Request Withdrawal</Text>
                </Pressable>
              </LinearGradient>

              {/* Transactions header */}
              <View style={s.sectionHeader}>
                <Text style={s.sectionTitle}>RECENT TRANSACTIONS</Text>
              </View>
            </>
          }
          renderItem={({ item }) => <TxRow tx={item} />}
          ListEmptyComponent={
            <View style={s.center}>
              <Ionicons name="wallet-outline" size={40} color="rgba(255,255,255,0.1)" />
              <Text style={s.emptyText}>No transactions yet</Text>
            </View>
          }
        />
      )}

      {wallet && (
        <WithdrawSheet
          wallet={wallet}
          visible={showWithdraw}
          onClose={() => setShowWithdraw(false)}
          onSuccess={(updated) => {
            setWallet(updated);
            setShowWithdraw(false);
          }}
        />
      )}
    </View>
  );
}

const s = StyleSheet.create({
  header:       { flexDirection: 'row', alignItems: 'center', paddingHorizontal: 16, paddingBottom: 12, borderBottomWidth: 1, borderBottomColor: BORDER },
  backBtn:      { width: 36, height: 36, borderRadius: 10, backgroundColor: GLASS, alignItems: 'center', justifyContent: 'center' },
  headerTitle:  { flex: 1, textAlign: 'center', fontSize: 17, fontWeight: '700', color: '#FFF', fontFamily: 'SpaceGrotesk-Bold' },
  center:       { flex: 1, alignItems: 'center', justifyContent: 'center', paddingTop: 60 },
  errorText:    { fontSize: 13, color: RED, fontFamily: 'JetBrainsMono-Regular', marginBottom: 12, textAlign: 'center', paddingHorizontal: 24 },
  retryBtn:     { backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 8, paddingHorizontal: 20, paddingVertical: 8 },
  retryText:    { fontSize: 13, color: CYAN, fontFamily: 'SpaceGrotesk-SemiBold' },
  balanceCard:  { margin: 16, padding: 24, borderRadius: 20, borderWidth: 1, borderColor: 'rgba(0,255,136,0.15)', alignItems: 'center', gap: 6 },
  balLabel:     { fontSize: 10, letterSpacing: 2, color: 'rgba(255,255,255,0.35)', fontFamily: 'JetBrainsMono-Regular' },
  balAmount:    { fontSize: 40, fontWeight: '700', color: GREEN, fontFamily: 'SpaceGrotesk-Bold' },
  balReserved:  { fontSize: 11, color: 'rgba(255,255,255,0.3)', fontFamily: 'JetBrainsMono-Regular' },
  withdrawBtn:  { flexDirection: 'row', alignItems: 'center', gap: 8, backgroundColor: 'rgba(0,255,136,0.08)', borderWidth: 1, borderColor: 'rgba(0,255,136,0.25)', borderRadius: 10, paddingHorizontal: 20, paddingVertical: 10, marginTop: 8 },
  withdrawText: { fontSize: 13, color: GREEN, fontFamily: 'SpaceGrotesk-SemiBold' },
  sectionHeader:{ paddingHorizontal: 16, paddingTop: 8, paddingBottom: 4 },
  sectionTitle: { fontSize: 10, letterSpacing: 1.5, color: 'rgba(255,255,255,0.3)', fontFamily: 'JetBrainsMono-Regular' },
  txRow:        { flexDirection: 'row', alignItems: 'center', gap: 12, paddingHorizontal: 16, paddingVertical: 14, borderBottomWidth: 1, borderBottomColor: BORDER },
  txIcon:       { width: 36, height: 36, borderRadius: 10, alignItems: 'center', justifyContent: 'center' },
  txDesc:       { fontSize: 13, color: '#FFF', fontFamily: 'SpaceGrotesk-SemiBold' },
  txDate:       { fontSize: 10, color: 'rgba(255,255,255,0.3)', fontFamily: 'JetBrainsMono-Regular', marginTop: 2 },
  txAmount:     { fontSize: 14, fontWeight: '700', fontFamily: 'JetBrainsMono-Regular' },
  emptyText:    { fontSize: 13, color: 'rgba(255,255,255,0.25)', fontFamily: 'JetBrainsMono-Regular', marginTop: 12 },
});

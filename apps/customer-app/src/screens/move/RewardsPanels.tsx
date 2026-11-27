/**
 * Move app — the rewards panels: loyalty tier, referrals, the company rate
 * and account credit. Offers shows the first three; Payments shows credit.
 *
 * Each panel loads its own slice and hides itself where promotions is not
 * deployed (the API returns null), so a missing feature is absent rather than
 * an error. Every number comes from the server.
 */
import React, { useCallback, useEffect, useState } from 'react';
import { ActivityIndicator, Pressable, Share, StyleSheet, Text, View } from 'react-native';
import {
  claimReferral, creditEntryTitle, getCorporate, getCredit, getLoyalty, getReferrals, linkCorporate, percent,
  referralShareMessage, tierPerk, tierProgress, unlinkCorporate,
  type Corporate, type Credit, type Loyalty, type Referrals,
} from '../../services/api/rewards';
import { formatMoney } from './format';
import { HEADING, M } from './theme';
import { Field, GhostButton, Label, Panel } from './ui';

function apiMessage(err: any): string {
  return err?.data?.error?.message ?? err?.message ?? 'Something went wrong.';
}

function day(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? '' : d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}

/** Loads one slice; `undefined` while loading, `null` where it isn't deployed. */
function useSlice<T>(load: () => Promise<T | null>) {
  const [value, setValue] = useState<T | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const reload = useCallback(async () => {
    try {
      setValue(await load());
      setError(null);
    } catch (err) {
      setError(apiMessage(err));
    }
  }, [load]);
  useEffect(() => { reload(); }, [reload]);
  return { value, setValue, error, reload };
}

// ── Loyalty ──────────────────────────────────────────────────────────────────

export function LoyaltyPanel({ currency, refreshKey }: { currency?: string; refreshKey?: number }) {
  const { value: loyalty, error, reload } = useSlice<Loyalty>(getLoyalty);
  useEffect(() => { if (refreshKey) reload(); }, [refreshKey, reload]);
  const money = (c: number) => formatMoney(c, currency, { whole: true });

  if (loyalty === null || (loyalty && !loyalty.enabled)) return null;
  if (error) return <><Label>Member tier</Label><Panel tone="amber"><Text style={s.body}>{error}</Text></Panel></>;
  if (!loyalty) return null;

  const progress = tierProgress(loyalty);
  return (
    <>
      <Label>Member tier</Label>
      <Panel tone={loyalty.tier ? 'accent' : 'default'}>
        <View style={s.rowBetween}>
          <Text style={s.tierName}>{loyalty.tier?.name ?? 'Not yet a member'}</Text>
          <Text style={s.kickerMuted}>{loyalty.moves} MOVE{loyalty.moves === 1 ? '' : 'S'} · 12 MO</Text>
        </View>
        {loyalty.tier && <Text style={s.perk}>{tierPerk(loyalty.tier, money)}</Text>}

        {loyalty.next && (
          <>
            <View
              style={s.track}
              accessibilityRole="progressbar"
              accessibilityValue={{ min: 0, max: 100, now: Math.round(progress * 100) }}
            >
              <View style={[s.fill, { width: `${Math.round(progress * 100)}%` }]} />
            </View>
            <Text style={s.body}>
              {loyalty.moves_to_next ?? Math.max(0, loyalty.next.min_moves - loyalty.moves)} more
              {' '}move{(loyalty.moves_to_next ?? 0) === 1 ? '' : 's'} to {loyalty.next.name} — {tierPerk(loyalty.next, money)}.
            </Text>
          </>
        )}

        {loyalty.ladder.length > 0 && (
          <View style={s.ladder}>
            {loyalty.ladder.map((t) => {
              const here = loyalty.tier?.name === t.name;
              return (
                <View key={t.name} style={s.ladderRow}>
                  <View style={[s.rung, here && s.rungOn]} />
                  <View style={{ flex: 1 }}>
                    <Text style={[s.ladderName, here && { color: M.accent }]}>{t.name}</Text>
                    <Text style={s.note}>{tierPerk(t, money)}{t.referral_multiplier > 1 ? ` · referrals ×${t.referral_multiplier}` : ''}</Text>
                  </View>
                  <Text style={s.note}>{t.min_moves}+</Text>
                </View>
              );
            })}
          </View>
        )}
        <Text style={[s.note, { marginTop: 10 }]}>Completed moves in the last 12 months. Tier discounts come off extras, and apply on their own — no code needed.</Text>
      </Panel>
    </>
  );
}

// ── Referrals ────────────────────────────────────────────────────────────────

export function ReferralPanel({ currency }: { currency?: string }) {
  const { value: mine, error, reload } = useSlice<Referrals>(getReferrals);
  const [draft, setDraft] = useState('');
  const [claiming, setClaiming] = useState(false);
  const [claimMsg, setClaimMsg] = useState<{ ok: boolean; text: string } | null>(null);

  if (mine === null) return null;
  const money = (c: number) => formatMoney(c, mine?.currency ?? currency, { whole: true });

  async function claim() {
    const code = draft.trim();
    if (!code || claiming) return;
    setClaiming(true);
    try {
      const r = await claimReferral(code);
      setClaimMsg(r.ok
        ? { ok: true, text: "You're linked. Your friend is credited once your first move is done." }
        : { ok: false, text: r.message ?? "That code can't be used." });
      if (r.ok) { setDraft(''); reload(); }
    } catch (err) {
      setClaimMsg({ ok: false, text: apiMessage(err) });
    } finally {
      setClaiming(false);
    }
  }

  const rewarded = mine?.invitees.filter((i) => i.rewarded).length ?? 0;
  return (
    <>
      <Label>Invite friends</Label>
      <Panel>
        {error && <Text style={[s.body, { color: M.amberText }]}>{error}</Text>}
        {!mine && !error && <ActivityIndicator color={M.accent} />}
        {mine && (
          <>
            <Text style={s.body}>
              {mine.reward_cents > 0
                ? `Share your code. When a friend's first move is done, you get ${money(mine.reward_cents)} of credit.`
                : 'Share your code with friends who are moving.'}
            </Text>
            <View style={[s.rowBetween, { marginTop: 12 }]}>
              <View style={s.codeChip}><Text style={s.codeText} selectable>{mine.code}</Text></View>
              <Pressable
                onPress={() => Share.share({ message: referralShareMessage(mine.code) }).catch(() => {})}
                accessibilityRole="button"
                style={({ pressed }) => [s.shareBtn, pressed && { opacity: 0.7 }]}
              >
                <Text style={s.shareText}>SHARE</Text>
              </Pressable>
            </View>
            {mine.invitees.length > 0 && (
              <View style={{ marginTop: 14, gap: 8 }}>
                <Text style={s.kickerMuted}>{mine.invitees.length} JOINED · {rewarded} REWARDED</Text>
                {mine.invitees.slice(0, 5).map((i, n) => (
                  <View key={n} style={s.rowBetween}>
                    <Text style={s.note}>Joined {day(i.joined_at)}</Text>
                    <Text style={[s.note, i.rewarded && { color: M.accent }]}>
                      {i.rewarded ? `+${money(i.reward_cents ?? 0)}` : 'First move pending'}
                    </Text>
                  </View>
                ))}
              </View>
            )}
          </>
        )}
        <View style={s.claimRow}>
          <Field
            label="Got a friend's code?"
            value={draft}
            onChangeText={(t) => { setDraft(t.toUpperCase()); setClaimMsg(null); }}
            autoCapitalize="characters"
            autoCorrect={false}
            placeholder="K7M2QP9X"
            returnKeyType="done"
            onSubmitEditing={claim}
          />
          <GhostButton label={claiming ? 'Linking…' : 'Link'} onPress={claim} color={M.accent} />
        </View>
        {claimMsg && <Text style={[s.body, { marginTop: 8, color: claimMsg.ok ? M.accent : M.amberText }]}>{claimMsg.text}</Text>}
      </Panel>
    </>
  );
}

// ── Company rate ─────────────────────────────────────────────────────────────

export function CorporatePanel({ onChanged }: { onChanged?: () => void }) {
  const { value: corp, setValue, error } = useSlice<Corporate>(getCorporate);
  const [draft, setDraft] = useState('');
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  if (corp === null) return null;

  async function link(code?: string) {
    if (busy) return;
    setBusy(true);
    setMsg(null);
    try {
      setValue(await linkCorporate(code));
      setDraft('');
      onChanged?.();
    } catch (err: any) {
      setMsg(err?.status === 404 ? "That company code isn't one we know." : apiMessage(err));
    } finally {
      setBusy(false);
    }
  }

  async function unlink() {
    if (busy) return;
    setBusy(true);
    try {
      await unlinkCorporate();
      setValue({ linked: null, domain_match: corp?.domain_match ?? null });
      onChanged?.();
    } catch (err) {
      setMsg(apiMessage(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <Label>Company rate</Label>
      <Panel tone={corp?.linked ? 'accent' : 'default'}>
        {error && <Text style={[s.body, { color: M.amberText }]}>{error}</Text>}
        {!corp && !error && <ActivityIndicator color={M.accent} />}
        {corp?.linked && (
          <>
            <View style={s.rowBetween}>
              <Text style={s.tierName}>{corp.linked.firm_name}</Text>
              <Text style={s.kicker}>{percent(corp.linked.percent_bps)} OFF CARRIAGE</Text>
            </View>
            <Text style={[s.body, { marginTop: 6 }]}>
              Applied to every move. A promo code is used instead only when it takes off more.
            </Text>
            <GhostButton label={busy ? 'Unlinking…' : 'Unlink'} onPress={unlink} style={{ marginTop: 12, alignSelf: 'flex-start' }} />
          </>
        )}
        {corp && !corp.linked && (
          <>
            <Text style={s.body}>Moving for work? Link your company to move at its rate.</Text>
            {!!corp.domain_match && (
              <GhostButton
                label={busy ? 'Linking…' : `Link ${corp.domain_match} — matches your email`}
                onPress={() => link()}
                color={M.accent}
                style={{ marginTop: 12 }}
              />
            )}
            <View style={s.claimRow}>
              <Field
                label="Company code"
                value={draft}
                onChangeText={(t) => { setDraft(t.toUpperCase()); setMsg(null); }}
                autoCapitalize="characters"
                autoCorrect={false}
                placeholder="ACME-CORP"
                returnKeyType="done"
                onSubmitEditing={() => draft.trim() && link(draft)}
              />
              <GhostButton label="Link" onPress={() => draft.trim() && link(draft)} color={M.accent} />
            </View>
          </>
        )}
        {!!msg && <Text style={[s.body, { marginTop: 8, color: M.amberText }]}>{msg}</Text>}
      </Panel>
    </>
  );
}

// ── Credit ───────────────────────────────────────────────────────────────────

export function CreditPanel() {
  const { value: credit, error } = useSlice<Credit>(getCredit);
  const [showAll, setShowAll] = useState(false);
  if (credit === null) return null;
  if (!credit && !error) return null;

  const money = (c: number) => formatMoney(Math.abs(c), credit?.currency, { whole: false });
  const entries = credit ? (showAll ? credit.entries : credit.entries.slice(0, 4)) : [];
  return (
    <Panel tone={credit && credit.balance_cents > 0 ? 'accent' : 'default'} style={{ marginTop: 20 }}>
      <Text style={s.kickerMuted}>ACCOUNT CREDIT</Text>
      {error && <Text style={[s.body, { color: M.amberText }]}>{error}</Text>}
      {credit && (
        <>
          <Text style={s.creditHero}>{formatMoney(credit.balance_cents, credit.currency, { whole: false })}</Text>
          <Text style={s.note}>
            Comes off your next move by itself, after any other discount. It can't be withdrawn or topped up.
          </Text>
          {entries.length > 0 && (
            <View style={{ marginTop: 12, gap: 10 }}>
              {entries.map((e, i) => (
                <View key={i} style={s.rowBetween}>
                  <View style={{ flex: 1, paddingRight: 10 }}>
                    <Text style={s.entryTitle} numberOfLines={1}>{creditEntryTitle(e)}</Text>
                    <Text style={s.note}>{day(e.created_at)}</Text>
                  </View>
                  <Text style={[s.entryAmount, { color: e.amount_cents > 0 ? M.accent : M.muted }]}>
                    {e.amount_cents > 0 ? '+' : '−'}{money(e.amount_cents)}
                  </Text>
                </View>
              ))}
            </View>
          )}
          {credit.entries.length > 4 && (
            <GhostButton label={showAll ? 'Show less' : `All ${credit.entries.length}`} onPress={() => setShowAll(!showAll)} style={{ marginTop: 10, alignSelf: 'flex-start' }} />
          )}
        </>
      )}
    </Panel>
  );
}

const s = StyleSheet.create({
  body:        { fontSize: 13, lineHeight: 19, color: M.muted },
  note:        { fontSize: 11, lineHeight: 16, color: M.faint },
  kicker:      { fontSize: 11, letterSpacing: 1.4, fontWeight: '700', color: M.accent },
  kickerMuted: { fontSize: 11, letterSpacing: 1.4, color: M.label },
  rowBetween:  { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', gap: 10, flexWrap: 'wrap' },
  tierName:    { fontFamily: HEADING, fontWeight: '700', fontSize: 20, color: M.ink, flexShrink: 1 },
  perk:        { fontSize: 14, color: M.accentText, marginTop: 2 },
  track:       { height: 6, borderRadius: 3, backgroundColor: M.hairline, overflow: 'hidden', marginTop: 14, marginBottom: 8 },
  fill:        { height: 6, borderRadius: 3, backgroundColor: M.accent },
  ladder:      { marginTop: 14, gap: 10, borderTopWidth: 1, borderTopColor: M.hairline, paddingTop: 12 },
  ladderRow:   { flexDirection: 'row', alignItems: 'center', gap: 10 },
  rung:        { width: 8, height: 8, borderRadius: 4, borderWidth: 1, borderColor: M.hairlineStrong },
  rungOn:      { backgroundColor: M.accent, borderColor: M.accent },
  ladderName:  { fontSize: 13, color: M.ink, fontWeight: '600' },
  codeChip:    { borderWidth: 1, borderStyle: 'dashed', borderColor: M.accentBorder, borderRadius: 8, paddingHorizontal: 12, paddingVertical: 6 },
  codeText:    { color: M.accent, fontWeight: '700', letterSpacing: 1.6, fontSize: 16, fontVariant: ['tabular-nums'] },
  shareBtn:    { minHeight: 44, minWidth: 96, paddingHorizontal: 16, borderRadius: 12, backgroundColor: M.accentTintStrong, borderWidth: 1, borderColor: M.accentBorder, alignItems: 'center', justifyContent: 'center' },
  shareText:   { color: M.accent, fontWeight: '700', letterSpacing: 1.2, fontSize: 13 },
  claimRow:    { flexDirection: 'row', alignItems: 'flex-end', gap: 10, marginTop: 14 },
  creditHero:  { fontFamily: HEADING, fontWeight: '700', fontSize: 30, color: M.ink, marginTop: 4, marginBottom: 4 },
  entryTitle:  { fontSize: 13, color: M.ink },
  entryAmount: { fontSize: 13, fontWeight: '600', fontVariant: ['tabular-nums'] },
});

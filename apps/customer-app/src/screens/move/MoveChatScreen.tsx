/**
 * Move app — the thread with your driver.
 *
 * Messages are engagement's; who may read or post is decided there, against
 * order-intake and driver-ops. The thread polls while it is on screen and marks
 * itself read; there is no push yet, so a closed app learns nothing until it
 * is opened again.
 */
import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  ActivityIndicator, Alert, KeyboardAvoidingView, Platform, Pressable, ScrollView,
  StyleSheet, Text, TextInput, View,
} from 'react-native';
import { useIsFocused } from '@react-navigation/native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Ionicons } from '@expo/vector-icons';
import {
  callMessage, CHAT_POLL_MS, listMessages, makeClientMessageId, markThreadRead, mergeMessages,
  sendMessage, startCall, type JobMessage,
} from '../../services/api/chat';
import { initials } from './format';
import { HEADING, M } from './theme';
import { Ambient } from './ui';

const QUICK_REPLIES = ['I’m at the door', 'Where are you?', 'Please call me', 'Leave it with the concierge'];

function when(iso: string): string {
  const at = new Date(iso);
  return Number.isNaN(at.getTime()) ? '' : at.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
}

function apiMessage(err: any): string {
  if (err?.status === 403) return 'This thread is only for you and your driver.';
  return err?.data?.error?.message ?? err?.message ?? 'Something went wrong.';
}

export function MoveChatScreen({ navigation, route }: { navigation: any; route: any }) {
  const insets = useSafeAreaInsets();
  const isFocused = useIsFocused();
  const shipmentId: string | undefined = route.params?.id;
  const driverName: string | undefined = route.params?.driverName;
  const driverPhone: string | undefined = route.params?.driverPhone;

  const [messages, setMessages] = useState<JobMessage[]>([]);
  const [canSend, setCanSend] = useState(true);
  const [draft, setDraft] = useState('');
  const [sending, setSending] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [calling, setCalling] = useState(false);
  const scroller = useRef<ScrollView>(null);

  const load = useCallback(async () => {
    if (!shipmentId) return;
    try {
      const thread = await listMessages(shipmentId);
      setMessages((prev) => mergeMessages(prev, thread.messages));
      setCanSend(thread.can_send);
      setError(null);
    } catch (err) {
      setError(apiMessage(err));
    } finally {
      setLoading(false);
    }
  }, [shipmentId]);

  useEffect(() => { void load(); }, [load]);

  useEffect(() => {
    if (!isFocused || !shipmentId) return;
    const timer = setInterval(() => { void load(); }, CHAT_POLL_MS);
    return () => clearInterval(timer);
  }, [isFocused, shipmentId, load]);

  // Reading it is reading it: the badge on the tracking screen clears here.
  useEffect(() => {
    if (!isFocused || !shipmentId || messages.length === 0) return;
    markThreadRead(shipmentId).catch(() => {});
  }, [isFocused, shipmentId, messages.length]);

  // The platform rings this phone first, then the driver, showing its own
  // number to both. The driver's line is never sent to this app.
  const placeCall = useCallback(async () => {
    if (!shipmentId || calling) return;
    setCalling(true);
    try {
      const attempt = await startCall(shipmentId);
      Alert.alert(attempt.bridged ? 'Connecting you' : 'Call not available', callMessage(attempt));
    } catch (err) {
      Alert.alert("Couldn't call", apiMessage(err));
    } finally {
      setCalling(false);
    }
  }, [shipmentId, calling]);

  const send = useCallback(async (text: string) => {
    const body = text.trim();
    if (!body || !shipmentId || sending) return;
    setSending(true);
    try {
      const message = await sendMessage(shipmentId, body, makeClientMessageId());
      setMessages((prev) => mergeMessages(prev, [message]));
      setDraft('');
      setError(null);
    } catch (err) {
      setError(apiMessage(err));
    } finally {
      setSending(false);
    }
  }, [shipmentId, sending]);

  return (
    <KeyboardAvoidingView style={s.root} behavior={Platform.OS === 'ios' ? 'padding' : undefined}>
      <Ambient />

      <View style={[s.header, { paddingTop: insets.top + 10 }]}>
        <Pressable
          onPress={() => navigation.goBack()}
          hitSlop={8}
          accessibilityRole="button"
          accessibilityLabel="Back"
          style={({ pressed }) => [s.iconBtn, pressed && { transform: [{ scale: 0.92 }] }]}
        >
          <Ionicons name="chevron-back" size={22} color="rgba(242,246,250,0.8)" />
        </Pressable>
        <View style={s.avatar}><Text style={s.avatarText}>{initials(driverName ?? 'Driver')}</Text></View>
        <View style={{ flex: 1 }}>
          <Text style={s.who} numberOfLines={1}>{driverName ?? 'Your driver'}</Text>
          <Text style={s.presence}>On this move</Text>
        </View>
        <Pressable
          onPress={placeCall}
          disabled={calling}
          hitSlop={8}
          accessibilityRole="button"
          accessibilityLabel={`Call ${driverName ?? 'your driver'} on a masked line`}
          style={({ pressed }) => [s.iconBtn, s.callBtn, pressed && { transform: [{ scale: 0.92 }] }]}
        >
          {calling
            ? <ActivityIndicator color={M.accent} size="small" />
            : <Ionicons name="call-outline" size={18} color={M.accent} />}
        </Pressable>
      </View>

      <ScrollView
        ref={scroller}
        contentContainerStyle={{ paddingHorizontal: 20, paddingTop: 16, paddingBottom: 12, gap: 12 }}
        onContentSizeChange={() => scroller.current?.scrollToEnd({ animated: true })}
        keyboardShouldPersistTaps="handled"
      >
        <Text style={s.context}>Messages on this move stay with it</Text>

        {loading && messages.length === 0 && <ActivityIndicator color={M.accent} style={{ marginTop: 20 }} />}

        {!loading && messages.length === 0 && !error && (
          <Text style={s.empty}>Nothing yet. Say what your driver needs to know — a gate code, where to park.</Text>
        )}

        {messages.map((message) => {
          const mine = message.sender_role === 'customer';
          return (
            <View key={message.id} style={[s.row, mine ? s.rowMine : s.rowTheirs]}>
              <View style={[s.bubble, mine ? s.bubbleMine : s.bubbleTheirs]}>
                <Text style={[s.body, mine && { color: M.accentInk }]}>{message.body}</Text>
              </View>
              <Text style={[s.time, mine && { textAlign: 'right' }]}>{when(message.created_at)}</Text>
            </View>
          );
        })}

        {!!error && <Text style={s.error}>{error}</Text>}
      </ScrollView>

      {canSend ? (
        <View style={[s.composer, { paddingBottom: insets.bottom + 14 }]}>
          <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={s.quick}>
            {QUICK_REPLIES.map((reply) => (
              <Pressable
                key={reply}
                onPress={() => send(reply)}
                disabled={sending}
                accessibilityRole="button"
                style={({ pressed }) => [s.chip, pressed && { transform: [{ scale: 0.95 }] }]}
              >
                <Text style={s.chipText}>{reply}</Text>
              </Pressable>
            ))}
          </ScrollView>
          <View style={s.inputRow}>
            <TextInput
              value={draft}
              onChangeText={setDraft}
              placeholder={`Message ${driverName?.split(' ')[0] ?? 'your driver'}`}
              placeholderTextColor={M.faint}
              multiline
              style={s.input}
              accessibilityLabel="Your message"
            />
            <Pressable
              onPress={() => send(draft)}
              disabled={!draft.trim() || sending}
              accessibilityRole="button"
              accessibilityLabel="Send"
              style={({ pressed }) => [
                s.send,
                (!draft.trim() || sending) && { opacity: 0.45 },
                pressed && { transform: [{ scale: 0.92 }] },
              ]}
            >
              {sending
                ? <ActivityIndicator color={M.accentInk} size="small" />
                : <Ionicons name="send" size={18} color={M.accentInk} />}
            </Pressable>
          </View>
        </View>
      ) : (
        <View style={[s.closed, { paddingBottom: insets.bottom + 18 }]}>
          <Text style={s.closedText}>This move is finished. The messages stay here; new ones go to support.</Text>
          <Pressable onPress={() => navigation.navigate('Support')} accessibilityRole="button">
            <Text style={s.closedLink}>Message support</Text>
          </Pressable>
        </View>
      )}
    </KeyboardAvoidingView>
  );
}

const s = StyleSheet.create({
  root:         { flex: 1, backgroundColor: M.ground },
  header:       { flexDirection: 'row', alignItems: 'center', gap: 12, paddingHorizontal: 14, paddingBottom: 12, borderBottomWidth: 1, borderBottomColor: M.hairline },
  iconBtn:      { width: 44, height: 44, borderRadius: 14, alignItems: 'center', justifyContent: 'center' },
  callBtn:      { borderWidth: 1, borderColor: M.hairlineStrong },
  avatar:       { width: 36, height: 36, borderRadius: 18, alignItems: 'center', justifyContent: 'center', backgroundColor: M.accentTint, borderWidth: 1, borderColor: M.accentBorder },
  avatarText:   { fontFamily: HEADING, fontWeight: '700', fontSize: 14, color: M.accent },
  who:          { fontSize: 14, color: M.ink },
  presence:     { fontSize: 10, letterSpacing: 1.6, textTransform: 'uppercase', color: M.accent, marginTop: 2 },
  context:      { alignSelf: 'center', fontSize: 10, letterSpacing: 1.8, textTransform: 'uppercase', color: M.faint },
  empty:        { fontSize: 13, lineHeight: 20, color: M.muted, textAlign: 'center', marginTop: 24 },
  row:          { maxWidth: '82%' },
  rowMine:      { alignSelf: 'flex-end' },
  rowTheirs:    { alignSelf: 'flex-start' },
  bubble:       { paddingHorizontal: 15, paddingVertical: 12, borderRadius: 18, borderWidth: 1 },
  bubbleMine:   { backgroundColor: M.accent, borderColor: M.accent, borderBottomRightRadius: 5 },
  bubbleTheirs: { backgroundColor: 'rgba(255,255,255,0.05)', borderColor: M.hairlineStrong, borderBottomLeftRadius: 5 },
  body:         { fontSize: 14, lineHeight: 20, color: M.ink },
  time:         { fontSize: 10, color: M.faint, marginTop: 5 },
  error:        { fontSize: 12, lineHeight: 18, color: M.amberText, textAlign: 'center', marginTop: 8 },
  composer:     { paddingHorizontal: 14, paddingTop: 10, borderTopWidth: 1, borderTopColor: M.hairline, backgroundColor: M.sheet },
  quick:        { gap: 8, paddingBottom: 10, paddingHorizontal: 4 },
  chip:         { height: 38, paddingHorizontal: 14, borderRadius: 12, justifyContent: 'center', backgroundColor: 'rgba(255,255,255,0.045)', borderWidth: 1, borderColor: M.hairlineStrong },
  chipText:     { fontSize: 12, color: 'rgba(242,246,250,0.75)' },
  inputRow:     { flexDirection: 'row', alignItems: 'flex-end', gap: 10, paddingHorizontal: 4 },
  input:        { flex: 1, minHeight: 52, maxHeight: 120, paddingHorizontal: 16, paddingTop: 15, paddingBottom: 15, borderRadius: 16, fontSize: 14, color: M.ink, backgroundColor: 'rgba(255,255,255,0.05)', borderWidth: 1, borderColor: M.hairlineStrong },
  send:         { width: 52, height: 52, borderRadius: 16, alignItems: 'center', justifyContent: 'center', backgroundColor: M.accent },
  closed:       { paddingHorizontal: 20, paddingTop: 16, gap: 8, borderTopWidth: 1, borderTopColor: M.hairline, backgroundColor: M.sheet },
  closedText:   { fontSize: 13, lineHeight: 19, color: M.muted },
  closedLink:   { fontSize: 13, color: M.accent },
});

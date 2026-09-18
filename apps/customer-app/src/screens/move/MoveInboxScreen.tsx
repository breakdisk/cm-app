/**
 * Move app — Inbox. Every campaign message this account was sent, on any
 * channel, newest first. Opening one marks it read; "Mark all read" clears
 * the dots. A message with a deep link into the app offers to open it.
 */
import React, { useCallback, useState } from 'react';
import { ActivityIndicator, FlatList, Pressable, RefreshControl, StyleSheet, Text, View } from 'react-native';
import { useFocusEffect } from '@react-navigation/native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { getInbox, linkTarget, markAllRead, markRead, whenSent, type InboxItem } from '../../services/api/inbox';
import { M } from './theme';
import { Ambient, GhostButton, Panel, TopBar } from './ui';

export function MoveInboxScreen({ navigation }: { navigation: any }) {
  const insets = useSafeAreaInsets();
  const [items, setItems] = useState<InboxItem[] | null>(null);
  const [unread, setUnread] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [open, setOpen] = useState<string | null>(null);
  const [more, setMore] = useState(true);

  const load = useCallback(async () => {
    try {
      const inbox = await getInbox();
      setItems(inbox.items);
      setUnread(inbox.unread);
      setMore(inbox.items.length >= 50);
      setError(null);
    } catch (err: any) {
      setError(err?.message ?? "Couldn't load your inbox.");
      setItems((prev) => prev ?? []);
    }
  }, []);

  useFocusEffect(useCallback(() => { load(); }, [load]));

  async function loadMore() {
    if (!more || !items?.length) return;
    try {
      const page = await getInbox(items[items.length - 1].sent_at);
      setItems([...items, ...page.items]);
      setMore(page.items.length >= 50);
    } catch {
      setMore(false);
    }
  }

  function toggle(item: InboxItem) {
    setOpen(open === item.id ? null : item.id);
    if (!item.read_at) {
      const at = new Date().toISOString();
      setItems((list) => list?.map((i) => (i.id === item.id ? { ...i, read_at: at } : i)) ?? null);
      setUnread((n) => Math.max(0, n - 1));
      markRead(item.id).catch(() => {});
    }
  }

  async function readAll() {
    const at = new Date().toISOString();
    setItems((list) => list?.map((i) => (i.read_at ? i : { ...i, read_at: at })) ?? null);
    setUnread(0);
    try { await markAllRead(); } catch { load(); }
  }

  return (
    <View style={s.root}>
      <Ambient />
      <TopBar
        label="Inbox"
        onBack={() => navigation.goBack()}
        right={unread > 0 ? <GhostButton label="Mark all read" onPress={readAll} color={M.accent} /> : undefined}
      />
      {items === null ? (
        <ActivityIndicator color={M.accent} style={{ marginTop: 48 }} />
      ) : (
        <FlatList
          data={items}
          keyExtractor={(i) => i.id}
          contentContainerStyle={{ paddingHorizontal: 16, gap: 10, paddingBottom: insets.bottom + 32 }}
          refreshControl={<RefreshControl refreshing={refreshing} tintColor={M.accent} onRefresh={async () => { setRefreshing(true); await load(); setRefreshing(false); }} />}
          onEndReached={loadMore}
          onEndReachedThreshold={0.4}
          ListHeaderComponent={error ? <Panel tone="amber" style={{ marginBottom: 10 }}><Text style={s.body}>{error}</Text></Panel> : null}
          ListEmptyComponent={!error ? (
            <Panel><Text style={s.body}>Nothing here yet. Offers and news we send you land here too, so you can find them later.</Text></Panel>
          ) : null}
          renderItem={({ item }) => {
            const expanded = open === item.id;
            const target = linkTarget(item.deep_link);
            return (
              <Pressable
                onPress={() => toggle(item)}
                accessibilityRole="button"
                accessibilityState={{ expanded }}
                accessibilityLabel={`${item.read_at ? '' : 'Unread. '}${item.title}`}
                style={({ pressed }) => [s.card, !item.read_at && s.cardUnread, pressed && { opacity: 0.8 }]}
              >
                <View style={s.head}>
                  {!item.read_at && <View style={s.dot} />}
                  <Text style={[s.title, !item.read_at && { fontWeight: '700' }]} numberOfLines={expanded ? undefined : 1}>{item.title}</Text>
                  <Text style={s.when}>{whenSent(item.sent_at)}</Text>
                </View>
                <Text style={s.body} numberOfLines={expanded ? undefined : 2}>{item.body}</Text>
                {expanded && target && (
                  <GhostButton label="Open" onPress={() => navigation.navigate(target)} color={M.accent} style={{ marginTop: 10, alignSelf: 'flex-start' }} />
                )}
              </Pressable>
            );
          }}
        />
      )}
    </View>
  );
}

const s = StyleSheet.create({
  root:       { flex: 1, backgroundColor: M.ground },
  card:       { borderRadius: 16, borderWidth: 1, borderColor: M.hairline, backgroundColor: M.panel, padding: 14, gap: 6 },
  cardUnread: { borderColor: M.accentBorder, backgroundColor: M.accentTint },
  head:       { flexDirection: 'row', alignItems: 'center', gap: 8 },
  dot:        { width: 8, height: 8, borderRadius: 4, backgroundColor: M.accent },
  title:      { flex: 1, fontSize: 14, color: M.ink },
  when:       { fontSize: 11, color: M.faint, fontVariant: ['tabular-nums'] },
  body:       { fontSize: 13, lineHeight: 19, color: M.muted },
});

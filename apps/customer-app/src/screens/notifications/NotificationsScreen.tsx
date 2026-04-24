/**
 * Customer App — Notifications Screen
 * Shows the user's notification history from the engagement service.
 *   GET /v1/notifications?customer_id=<me>
 * Replaces the prior mock-data stub; falls back to an empty state if the
 * engagement service is unreachable.
 */
import React, { useCallback, useEffect, useState } from "react";
import { View, Text, StyleSheet, FlatList, Pressable, RefreshControl, ActivityIndicator } from "react-native";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";

import { FadeInView } from '../../components/FadeInView';
import { getStoredCustomerId } from '../../services/api/auth';
import { notificationsApi, type Notification, type NotificationChannel } from '../../services/api/notifications';

const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const AMBER  = "#FFAB00";
const PURPLE = "#A855F7";
const RED    = "#FF3B5C";
const BORDER = "rgba(255,255,255,0.08)";

// Channel → icon + color. Engagement service doesn't categorize notifications
// by "delivery / promo / loyalty / alert"; we derive visual intent from the
// channel. When a `category` field is added, switch on that instead.
const CHANNEL_CONFIG: Record<NotificationChannel, { icon: keyof typeof Ionicons.glyphMap; color: string }> = {
  WhatsApp: { icon: "chatbubble-outline",     color: GREEN  },
  Sms:      { icon: "phone-portrait-outline", color: CYAN   },
  Email:    { icon: "mail-outline",           color: PURPLE },
  Push:     { icon: "notifications-outline",  color: AMBER  },
};

function formatRelative(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "";
  const diffMin = Math.floor((Date.now() - then) / 60_000);
  if (diffMin < 1)    return "just now";
  if (diffMin < 60)   return `${diffMin}m ago`;
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 24)    return `${diffHr}h ago`;
  const diffDay = Math.floor(diffHr / 24);
  if (diffDay === 1)  return "Yesterday";
  if (diffDay < 7)    return `${diffDay} days ago`;
  return new Date(iso).toLocaleDateString();
}

// Engagement service doesn't track a read flag yet; treat anything within the
// last 24h as unread. When a `read_at` column + PATCH endpoint exists, switch.
function isUnread(n: Notification): boolean {
  const queued = new Date(n.queued_at).getTime();
  return !Number.isNaN(queued) && (Date.now() - queued) < 24 * 60 * 60 * 1000;
}

function deriveTitle(n: Notification): string {
  if (n.subject && n.subject.trim().length > 0) return n.subject;
  const firstLine = n.rendered_body.split('\n')[0] ?? '';
  return firstLine.length > 0 ? firstLine.slice(0, 80) : 'Notification';
}

export function NotificationsScreen() {
  const [notifications, setNotifications] = useState<Notification[]>([]);
  const [loading, setLoading]             = useState(true);
  const [refreshing, setRefreshing]       = useState(false);
  const [error, setError]                 = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const customerId = await getStoredCustomerId();
      if (!customerId) {
        setNotifications([]);
        return;
      }
      const resp = await notificationsApi.list({ customerId, limit: 50 });
      setNotifications(resp.notifications ?? []);
    } catch (e: unknown) {
      const err = e as { message?: string };
      setError(err?.message ?? 'Failed to load notifications');
    }
  }, []);

  useEffect(() => {
    (async () => {
      await load();
      setLoading(false);
    })();
  }, [load]);

  const onRefresh = useCallback(async () => {
    setRefreshing(true);
    await load();
    setRefreshing(false);
  }, [load]);

  const unreadCount = notifications.filter(isUnread).length;

  function renderItem({ item }: { item: Notification }) {
    const config = CHANNEL_CONFIG[item.channel] ?? CHANNEL_CONFIG.Push;
    const unread = isUnread(item);
    const title = deriveTitle(item);
    return (
      <FadeInView fromY={-16}>
        <Pressable style={({ pressed }) => [s.row, unread && s.rowUnread, { opacity: pressed ? 0.8 : 1 }]}>
          <View style={[s.iconWrap, { backgroundColor: config.color + "20" }]}>
            <Ionicons name={config.icon} size={18} color={config.color} />
            {unread && <View style={[s.unreadDot, { backgroundColor: config.color }]} />}
          </View>
          <View style={{ flex: 1 }}>
            <Text style={[s.title, unread && { color: "#FFF" }]}>{title}</Text>
            <Text style={s.body} numberOfLines={2}>{item.rendered_body}</Text>
            <Text style={s.time}>{formatRelative(item.queued_at)}</Text>
          </View>
        </Pressable>
      </FadeInView>
    );
  }

  return (
    <View style={s.container}>
      <LinearGradient colors={["rgba(0,229,255,0.08)", "transparent"]} style={s.hero}>
        <Text style={s.heroTitle}>Notifications</Text>
        {unreadCount > 0 && (
          <Text style={s.heroSub}>{unreadCount} unread</Text>
        )}
      </LinearGradient>

      {loading ? (
        <View style={s.center}>
          <ActivityIndicator color={CYAN} />
        </View>
      ) : error ? (
        <View style={s.center}>
          <Text style={s.errorText}>{error}</Text>
          <Pressable onPress={onRefresh} style={s.retryBtn}>
            <Text style={s.retryText}>Retry</Text>
          </Pressable>
        </View>
      ) : notifications.length === 0 ? (
        <View style={s.center}>
          <Ionicons name="notifications-off-outline" size={40} color="rgba(255,255,255,0.25)" />
          <Text style={s.emptyText}>No notifications yet.</Text>
          <Text style={s.emptySub}>Updates about your shipments and receipts will appear here.</Text>
        </View>
      ) : (
        <FlatList
          data={notifications}
          keyExtractor={(n) => n.id}
          renderItem={renderItem}
          contentContainerStyle={{ paddingBottom: 40 }}
          ItemSeparatorComponent={() => <View style={{ height: 1, backgroundColor: BORDER, marginHorizontal: 16 }} />}
          refreshControl={<RefreshControl refreshing={refreshing} onRefresh={onRefresh} tintColor={CYAN} />}
        />
      )}
    </View>
  );
}

const s = StyleSheet.create({
  container:  { flex: 1, backgroundColor: CANVAS },
  hero:       { paddingHorizontal: 20, paddingTop: 52, paddingBottom: 20 },
  heroTitle:  { fontSize: 26, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  heroSub:    { fontSize: 13, color: "rgba(255,255,255,0.4)", marginTop: 4 },
  row:        { flexDirection: "row", gap: 14, paddingHorizontal: 16, paddingVertical: 14, backgroundColor: "transparent" },
  rowUnread:  { backgroundColor: "rgba(0,229,255,0.04)" },
  iconWrap:   { position: "relative", width: 44, height: 44, borderRadius: 12, alignItems: "center", justifyContent: "center", flexShrink: 0 },
  unreadDot:  { position: "absolute", top: -3, right: -3, width: 8, height: 8, borderRadius: 4, borderWidth: 2, borderColor: CANVAS },
  title:      { fontSize: 13, fontWeight: "600", color: "rgba(255,255,255,0.85)", marginBottom: 3 },
  body:       { fontSize: 12, color: "rgba(255,255,255,0.4)", lineHeight: 17 },
  time:       { fontSize: 10, color: "rgba(255,255,255,0.2)", fontFamily: "JetBrainsMono-Regular", marginTop: 4 },
  center:     { flex: 1, alignItems: "center", justifyContent: "center", paddingHorizontal: 32, gap: 12 },
  errorText:  { color: RED, fontSize: 13, textAlign: "center" },
  emptyText:  { color: "rgba(255,255,255,0.6)", fontSize: 15, fontWeight: "600" },
  emptySub:   { color: "rgba(255,255,255,0.35)", fontSize: 12, textAlign: "center" },
  retryBtn:   { paddingHorizontal: 18, paddingVertical: 8, borderRadius: 10, borderWidth: 1, borderColor: BORDER },
  retryText:  { color: CYAN, fontSize: 13, fontWeight: "600" },
});

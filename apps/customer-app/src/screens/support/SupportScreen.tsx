/**
 * Customer App — Support Screen
 *
 * FAQ accordion + AI chat backed by the AI Intelligence Layer
 * (`POST /v1/agents/chat`). The agent runs Claude with a restricted
 * CustomerSupport tool allowlist and can look up the customer's shipments,
 * reschedule a delivery, and hand over to a human operator.
 *
 * The keyword-matching replies below are no longer the chat — they are the
 * offline/plan-gated fallback, used when the tenant's plan has no AI tier
 * (403) or the network call fails.
 */
import React, { useState, useRef, useCallback, useMemo } from "react";
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { FadeInView } from '../../components/FadeInView';
import {
  View, Text, StyleSheet, ScrollView, Pressable,
  TextInput, KeyboardAvoidingView, Platform, Linking, Alert,
} from "react-native";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";
import { useSelector } from "react-redux";
import { useNavigation, useFocusEffect } from "@react-navigation/native";
import type { RootState } from "../../store";
import {
  aiApi,
  AiUnavailableError,
  getStoredChatSession,
  storeChatSession,
  clearChatSession,
  type ChatShipmentContext,
} from "../../services/api/ai";

const CANVAS  = "#050810";
const CYAN    = "#00E5FF";
const GREEN   = "#00FF88";
const AMBER   = "#FFAB00";
const PURPLE  = "#A855F7";
const RED     = "#FF3B5C";
const GLASS   = "rgba(255,255,255,0.04)";
const BORDER  = "rgba(255,255,255,0.08)";

// ── FAQ data ─────────────────────────────────────────────────────────────────

interface FaqItem {
  q: string;
  a: string;
  icon: string;
  color: string;
}

const FAQS: FaqItem[] = [
  {
    q: "How do I track my shipment?",
    a: "Go to the Track tab and enter your AWB number (e.g. LS-A1B2C3D4). You'll see a live timeline of your package's journey including current location, driver details, and estimated delivery time.",
    icon: "cube-outline",
    color: CYAN,
  },
  {
    q: "What is a Balikbayan Box?",
    a: "A Balikbayan Box is a large freight shipment service for overseas workers sending goods home to the Philippines. It supports both Sea Freight (30–45 days, most economical) and Air Freight (5–10 days). A receiver passport copy is required for customs clearance.",
    icon: "globe-outline",
    color: PURPLE,
  },
  {
    q: "How does Cash on Delivery (COD) work?",
    a: "With COD, the recipient pays the declared amount in cash when the package is delivered to their door. The driver collects the payment and it is reconciled back to the merchant. COD is available for local shipments only.",
    icon: "cash-outline",
    color: AMBER,
  },
  {
    q: "What happens if delivery is attempted and I'm not home?",
    a: "Our driver will leave a notification and attempt re-delivery the next business day. You can reschedule via the app or contact support. After 3 failed attempts, the package may be returned to sender.",
    icon: "alert-circle-outline",
    color: AMBER,
  },
  {
    q: "How do I earn and use Loyalty Points?",
    a: "You earn 50 pts for every local booking and 150 pts for international. Points can be redeemed for shipping discounts. Reaching 1,000 pts unlocks Platinum tier with priority handling and free insurance.",
    icon: "star-outline",
    color: PURPLE,
  },
  {
    q: "How long does local delivery take?",
    a: "Same-city deliveries are typically completed within 1–3 business days. Same-day delivery is available in Metro Manila for bookings placed before 10 AM. Remote areas may take 3–7 days.",
    icon: "time-outline",
    color: GREEN,
  },
  {
    q: "What ID is accepted for KYC?",
    a: "Passport is accepted for both local and international shipments. Emirates ID is accepted for local shipments only. For international (Balikbayan Box) shipping, a valid Passport is mandatory for customs requirements.",
    icon: "card-outline",
    color: CYAN,
  },
  {
    q: "Can I cancel or change my booking?",
    a: "You can cancel a booking up to 1 hour after placing it, as long as it has not been picked up yet. Once in transit, cancellation is no longer possible. Contact support to modify delivery address.",
    icon: "close-circle-outline",
    color: RED,
  },
];

// ── Offline fallback replies ──────────────────────────────────────────────────
// Used only when the live agent is unreachable or not included in the plan.

const AI_RESPONSES: Record<string, string> = {
  track:       "To track your shipment, tap the **Track** tab and enter your AWB number (e.g. LS-A1B2C3D4). You'll see a full timeline with driver details and ETA.",
  balikbayan:  "Balikbayan Box is our international freight service. Sea freight takes 30–45 days (most economical) and air freight takes 5–10 days. A receiver's passport copy is required.",
  cod:         "Cash on Delivery means the recipient pays when the package arrives. The driver collects the cash, and it's remitted to the merchant. COD is for local shipments only.",
  delay:       "If your package is delayed, please check the tracking timeline first. Common reasons include traffic, hub sorting backlogs, or failed delivery attempts. Contact us if it's been more than 2 extra days.",
  points:      "You earn 50 loyalty points per local booking and 150 for international. Redeem points for discounts. 1,000 pts = Platinum tier with perks like priority handling.",
  cancel:      "You can cancel within 1 hour of booking if the package hasn't been picked up. Go to History, tap the shipment, and choose Cancel. After pickup, contact support.",
  default:     "I'm LogisticOS AI Support! I can help you with tracking, booking, Balikbayan Box, COD, loyalty points, and delivery issues. Try asking me something specific.",
};

function getAiReply(msg: string): string {
  const m = msg.toLowerCase();
  if (m.includes("track") || m.includes("awb"))          return AI_RESPONSES.track;
  if (m.includes("balikbayan") || m.includes("international") || m.includes("overseas")) return AI_RESPONSES.balikbayan;
  if (m.includes("cod") || m.includes("cash"))           return AI_RESPONSES.cod;
  if (m.includes("delay") || m.includes("late"))         return AI_RESPONSES.delay;
  if (m.includes("point") || m.includes("loyalty"))      return AI_RESPONSES.points;
  if (m.includes("cancel"))                              return AI_RESPONSES.cancel;
  return AI_RESPONSES.default;
}

// ── Sub-components ────────────────────────────────────────────────────────────

function FaqCard({ item }: { item: FaqItem }) {
  const [open, setOpen] = useState(false);
  return (
    <Pressable onPress={() => setOpen(v => !v)} style={[s.faqCard, open && { borderColor: item.color + "40" }]}>
      <View style={s.faqHeader}>
        <View style={[s.faqIcon, { backgroundColor: item.color + "15" }]}>
          <Ionicons name={item.icon as any} size={16} color={item.color} />
        </View>
        <Text style={s.faqQ}>{item.q}</Text>
        <Ionicons name={open ? "chevron-up" : "chevron-down"} size={14} color="rgba(255,255,255,0.3)" />
      </View>
      {open && (
        <FadeInView duration={200} style={s.faqBody}>
          <Text style={s.faqA}>{item.a}</Text>
        </FadeInView>
      )}
    </Pressable>
  );
}

interface ChatMsg {
  role: "user" | "ai";
  text: string;
  /** Rendered from the offline fallback rather than the live agent. */
  offline?: boolean;
  /** The agent handed this conversation to a human operator. */
  escalated?: boolean;
  /** Written by a human operator resolving the escalation, not by the agent. */
  fromOperator?: boolean;
}

// ── Main screen ───────────────────────────────────────────────────────────────

export function SupportScreen() {
  const insets     = useSafeAreaInsets();
  const navigation = useNavigation<any>();
  const name       = useSelector((s: RootState) => s.auth.name);
  const shipments  = useSelector((s: RootState) => s.shipments.list);

  const [tab,     setTab]     = useState<"faq" | "chat">("faq");
  const [input,   setInput]   = useState("");
  const [typing,  setTyping]  = useState(false);
  const [msgs,    setMsgs]    = useState<ChatMsg[]>([
    { role: "ai", text: `Hi ${name?.split(" ")[0] ?? "there"}! 👋 I'm the LogisticOS AI Support agent. How can I help you today?` },
  ]);
  /** Server-side conversation id — echoed back so the agent keeps context. */
  const [sessionId, setSessionId] = useState<string | undefined>(undefined);
  /** Sticky once the plan turns out not to include AI, so we stop retrying. */
  const [aiDisabled, setAiDisabled] = useState(false);
  const scrollRef = useRef<ScrollView>(null);

  /**
   * Shipment index passed with each turn. These all came from this user's own
   * authenticated session; the agent's tool calls are still executed under the
   * user's token, so this narrows what the agent talks about — it does not
   * widen what it can reach.
   */
  const shipmentContext: ChatShipmentContext[] = useMemo(
    () => shipments.slice(0, 20).map((s) => ({ id: s.id, awb: s.awb, status: s.status })),
    [shipments],
  );

  const send = useCallback(async (text: string) => {
    const trimmed = text.trim();
    if (!trimmed || typing) return;

    setMsgs(prev => [...prev, { role: "user", text: trimmed }]);
    setInput("");
    setTyping(true);

    // Plan has no AI tier — answer locally without a round-trip.
    if (aiDisabled) {
      setMsgs(prev => [...prev, { role: "ai", text: getAiReply(trimmed), offline: true }]);
      setTyping(false);
      return;
    }

    try {
      const res = await aiApi.chat({
        sessionId,
        message: trimmed,
        shipments: shipmentContext,
      });
      setSessionId(res.session_id);
      void storeChatSession(res.session_id);
      setMsgs(prev => [...prev, { role: "ai", text: res.reply, escalated: res.escalated }]);
    } catch (e: unknown) {
      if (e instanceof AiUnavailableError) {
        setAiDisabled(true);
        setMsgs(prev => [...prev, { role: "ai", text: getAiReply(trimmed), offline: true }]);
      } else {
        const err = e as { message?: string };
        setMsgs(prev => [...prev, {
          role: "ai",
          offline: true,
          text: `I couldn't reach support just now (${err?.message ?? "network error"}). Here's what I can tell you offline:\n\n${getAiReply(trimmed)}`,
        }]);
      }
    } finally {
      setTyping(false);
      setTimeout(() => scrollRef.current?.scrollToEnd({ animated: true }), 100);
    }
  }, [aiDisabled, sessionId, shipmentContext, typing]);

  /**
   * Pick up an operator's reply to an escalated conversation.
   *
   * The hand-off is asynchronous — ops answers via the admin portal minutes or
   * days later, and engagement pushes a notification. On every focus we check
   * the stored conversation: if a human has since resolved it, drop their reply
   * into the thread and retire the session so the next question starts fresh.
   */
  const syncEscalatedSession = useCallback(async () => {
    const stored = await getStoredChatSession();
    if (!stored) return;

    let state;
    try {
      state = await aiApi.getChat(stored);
    } catch {
      // Offline, or the session is gone//forbidden — leave the thread as-is.
      return;
    }

    if (state.resolved_by_human && state.latest_reply) {
      const answer = state.latest_reply;
      setMsgs(prev => {
        // The push can be delivered more than once — don't double-post.
        if (prev.some(m => m.role === "ai" && m.text === answer)) return prev;
        return [...prev, { role: "ai", text: answer, fromOperator: true }];
      });
      setTab("chat");
      setSessionId(undefined);
      void clearChatSession();
      return;
    }

    if (state.escalated) {
      // Still with a human — keep writing follow-ups onto the same case.
      setSessionId(prev => prev ?? stored);
      return;
    }

    // An ordinary finished conversation. Don't silently resume it on a fresh
    // launch: the agent would have context the customer can no longer see.
    if (!sessionId) void clearChatSession();
  }, [sessionId]);

  useFocusEffect(
    useCallback(() => {
      void syncEscalatedSession();
    }, [syncEscalatedSession]),
  );

  function sendMessage() {
    void send(input);
  }

  /** Jump to the chat tab and ask the agent something in one tap. */
  function askAgent(msg: string) {
    setTab("chat");
    void send(msg);
  }

  /**
   * Dial the tenant's support line. The number is deployment configuration, not
   * a constant — there is no platform-wide hotline, so with nothing configured
   * we route the customer to the agent instead of showing a dead button.
   */
  async function callSupport() {
    const phone = process.env.EXPO_PUBLIC_SUPPORT_PHONE;
    if (!phone) {
      askAgent("I'd like to speak to a human agent.");
      return;
    }
    const url = `tel:${phone}`;
    try {
      const can = await Linking.canOpenURL(url);
      if (can) await Linking.openURL(url);
      else throw new Error("No dialer available");
    } catch {
      Alert.alert("Call support", `Reach us on ${phone}, or use AI Chat for an instant answer.`);
    }
  }

  const QUICK_PROMPTS = [
    { label: "Where is my parcel?", msg: "Where is my parcel right now?" },
    { label: "Reschedule delivery", msg: "I need to reschedule my delivery." },
    { label: "Balikbayan Box",      msg: "Tell me about Balikbayan Box" },
    { label: "Talk to a human",     msg: "I'd like to speak to a human agent." },
  ];

  return (
    <KeyboardAvoidingView
      style={{ flex: 1, backgroundColor: CANVAS }}
      behavior={Platform.OS === "ios" ? "padding" : "height"}
      keyboardVerticalOffset={Platform.OS === "ios" ? insets.top + 8 : 0}
    >
      {/* Hero */}
      <LinearGradient colors={["rgba(255,171,0,0.09)", "transparent"]} style={s.hero}>
        <FadeInView fromY={-16}>
          <Text style={s.heroTitle}>Help & Support</Text>
          <Text style={s.heroSub}>FAQ or chat with our AI agent</Text>
        </FadeInView>
      </LinearGradient>

      {/* Tab switcher */}
      <FadeInView delay={60} fromY={-16} style={s.tabRow}>
        <Pressable onPress={() => setTab("faq")} style={[s.tabBtn, tab === "faq" && s.tabBtnActive]}>
          <Ionicons name="help-circle-outline" size={15} color={tab === "faq" ? AMBER : "rgba(255,255,255,0.35)"} />
          <Text style={[s.tabBtnText, { color: tab === "faq" ? AMBER : "rgba(255,255,255,0.35)" }]}>FAQ</Text>
        </Pressable>
        <Pressable onPress={() => setTab("chat")} style={[s.tabBtn, tab === "chat" && s.tabBtnActiveChat]}>
          <Ionicons name="chatbubble-ellipses-outline" size={15} color={tab === "chat" ? CYAN : "rgba(255,255,255,0.35)"} />
          <Text style={[s.tabBtnText, { color: tab === "chat" ? CYAN : "rgba(255,255,255,0.35)" }]}>AI Chat</Text>
          <View style={s.aiBadge}><Text style={s.aiBadgeText}>AI</Text></View>
        </Pressable>
      </FadeInView>

      {/* ── FAQ ── */}
      {tab === "faq" && (
        <ScrollView contentContainerStyle={{ paddingHorizontal: 16, paddingBottom: 40, gap: 8 }}>
          {/* Quick links */}
          <FadeInView delay={80} fromY={16} style={s.quickLinks}>
            {[
              {
                icon: "cube-outline", label: "Track Parcel", color: CYAN,
                onPress: () => navigation.navigate("Track"),
              },
              {
                icon: "alert-circle-outline", label: "Report Issue", color: RED,
                onPress: () => askAgent("I want to report a problem with my delivery."),
              },
              {
                icon: "refresh-circle-outline", label: "Reschedule", color: GREEN,
                onPress: () => askAgent("I need to reschedule my delivery."),
              },
              {
                icon: "call-outline", label: "Call Us", color: AMBER,
                onPress: () => { void callSupport(); },
              },
            ].map((q) => (
              <Pressable
                key={q.label}
                onPress={q.onPress}
                style={({ pressed }) => [s.quickLink, { opacity: pressed ? 0.7 : 1 }]}
              >
                <View style={[s.quickLinkIcon, { backgroundColor: q.color + "18" }]}>
                  <Ionicons name={q.icon as any} size={18} color={q.color} />
                </View>
                <Text style={s.quickLinkText}>{q.label}</Text>
              </Pressable>
            ))}
          </FadeInView>

          <Text style={s.sectionLabel}>Frequently Asked Questions</Text>
          {FAQS.map((faq, i) => (
            <FadeInView key={i} delay={i * 30} fromY={16}>
              <FaqCard item={faq} />
            </FadeInView>
          ))}

          {/* Contact strip — Live Chat jumps to the AI chat tab (same screen);
              Email opens the OS mail composer via Linking. Engagement service
              doesn't have a support-ticket concept yet, so email is the
              system of record for human escalation. */}
          <FadeInView delay={200} fromY={16} style={s.contactRow}>
            <Pressable
              onPress={() => setTab("chat")}
              style={({ pressed }) => [s.contactItem, { opacity: pressed ? 0.7 : 1 }]}
            >
              <Ionicons name="chatbubble-outline" size={18} color={CYAN} />
              <Text style={s.contactLabel}>AI Chat</Text>
              <Text style={s.contactSub}>Instant answers</Text>
            </Pressable>
            <View style={[s.contactDivider]} />
            <Pressable
              onPress={async () => {
                const url = "mailto:support@cargomarket.net?subject=CargoMarket%20App%20Support";
                try {
                  const can = await Linking.canOpenURL(url);
                  if (can) await Linking.openURL(url);
                  else throw new Error("No mail app available");
                } catch {
                  Alert.alert("Email support", "Send a message to support@cargomarket.net");
                }
              }}
              style={({ pressed }) => [s.contactItem, { opacity: pressed ? 0.7 : 1 }]}
            >
              <Ionicons name="mail-outline" size={18} color={PURPLE} />
              <Text style={s.contactLabel}>Email</Text>
              <Text style={s.contactSub}>support@cargomarket.net</Text>
            </Pressable>
          </FadeInView>
        </ScrollView>
      )}

      {/* ── AI Chat ── */}
      {tab === "chat" && (
        <View style={{ flex: 1 }}>
          <ScrollView
            ref={scrollRef}
            contentContainerStyle={{ paddingHorizontal: 16, paddingTop: 16, paddingBottom: 8, gap: 10 }}
            onContentSizeChange={() => scrollRef.current?.scrollToEnd({ animated: true })}
            keyboardShouldPersistTaps="handled"
          >
            {msgs.map((m, i) => (
              <FadeInView key={i} style={[s.msgRow, m.role === "user" ? s.msgRowUser : s.msgRowAi]}>
                {m.role === "ai" && (
                  <View style={[s.aiBubbleIcon, m.fromOperator && { backgroundColor: GREEN + "18" }]}>
                    <Ionicons
                      name={m.fromOperator ? "person" : "logo-electron"}
                      size={14}
                      color={m.fromOperator ? GREEN : CYAN}
                    />
                  </View>
                )}
                {m.role === "user" ? (
                  <LinearGradient colors={[CYAN, PURPLE]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 1 }} style={[s.bubble, s.bubbleUser]}>
                    <Text style={[s.bubbleText, { color: CANVAS }]}>{m.text}</Text>
                  </LinearGradient>
                ) : (
                  <View style={[
                    s.bubble,
                    s.bubbleAi,
                    m.escalated && s.bubbleEscalated,
                    m.fromOperator && s.bubbleOperator,
                  ]}>
                    {m.fromOperator && (
                      <Text style={s.operatorLabel}>Support team</Text>
                    )}
                    <Text style={s.bubbleText}>{m.text}</Text>
                    {m.escalated && (
                      <View style={s.bubbleTag}>
                        <Ionicons name="person-outline" size={10} color={AMBER} />
                        <Text style={[s.bubbleTagText, { color: AMBER }]}>Handed to a human agent</Text>
                      </View>
                    )}
                    {m.offline && (
                      <View style={s.bubbleTag}>
                        <Ionicons name="cloud-offline-outline" size={10} color="rgba(255,255,255,0.3)" />
                        <Text style={s.bubbleTagText}>Offline answer</Text>
                      </View>
                    )}
                  </View>
                )}
              </FadeInView>
            ))}
            {typing && (
              <FadeInView duration={200} style={[s.msgRow, s.msgRowAi]}>
                <View style={s.aiBubbleIcon}>
                  <Ionicons name="logo-electron" size={14} color={CYAN} />
                </View>
                <View style={[s.bubble, s.bubbleAi]}>
                  <Text style={s.typingDots}>• • •</Text>
                </View>
              </FadeInView>
            )}
          </ScrollView>

          {/* Quick prompt chips */}
          <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={s.promptChips}>
            {QUICK_PROMPTS.map((p) => (
              <Pressable
                key={p.label}
                onPress={() => { void send(p.msg); }}
                disabled={typing}
                style={({ pressed }) => [s.promptChip, { opacity: pressed || typing ? 0.5 : 1 }]}
              >
                <Text style={s.promptChipText}>{p.label}</Text>
              </Pressable>
            ))}
          </ScrollView>

          {/* Input bar — extra bottom padding for Android gesture nav bar */}
          <View style={[s.inputBar, { paddingBottom: Math.max(12, insets.bottom + 4) }]}>
            <TextInput
              value={input}
              onChangeText={setInput}
              placeholder="Ask me anything..."
              placeholderTextColor="rgba(255,255,255,0.2)"
              style={s.chatInput}
              returnKeyType="send"
              onSubmitEditing={sendMessage}
              multiline
            />
            <Pressable
              onPress={sendMessage}
              disabled={!input.trim() || typing}
              style={({ pressed }) => [s.sendBtn, { opacity: pressed || !input.trim() ? 0.5 : 1 }]}
            >
              <LinearGradient colors={[CYAN, PURPLE]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 1 }} style={s.sendBtnGrad}>
                <Ionicons name="send" size={16} color={CANVAS} />
              </LinearGradient>
            </Pressable>
          </View>
        </View>
      )}
    </KeyboardAvoidingView>
  );
}

const s = StyleSheet.create({
  hero:           { paddingHorizontal: 20, paddingTop: 52, paddingBottom: 16 },
  heroTitle:      { fontSize: 26, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  heroSub:        { fontSize: 13, color: "rgba(255,255,255,0.4)", marginTop: 4 },

  tabRow:         { flexDirection: "row", marginHorizontal: 16, marginBottom: 16, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 12, padding: 4, gap: 4 },
  tabBtn:         { flex: 1, flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 6, paddingVertical: 9, borderRadius: 8 },
  tabBtnActive:   { backgroundColor: "rgba(255,171,0,0.10)", borderWidth: 1, borderColor: "rgba(255,171,0,0.25)" },
  tabBtnActiveChat:{ backgroundColor: "rgba(0,229,255,0.08)", borderWidth: 1, borderColor: "rgba(0,229,255,0.2)" },
  tabBtnText:     { fontSize: 13, fontFamily: "SpaceGrotesk-SemiBold" },
  aiBadge:        { paddingHorizontal: 5, paddingVertical: 1, borderRadius: 4, backgroundColor: CYAN + "25" },
  aiBadgeText:    { fontSize: 8, fontFamily: "JetBrainsMono-Regular", color: CYAN, letterSpacing: 0.5 },

  quickLinks:     { flexDirection: "row", gap: 10, marginBottom: 8 },
  quickLink:      { flex: 1, alignItems: "center", gap: 6 },
  quickLinkIcon:  { width: 48, height: 48, borderRadius: 14, alignItems: "center", justifyContent: "center", borderWidth: 1, borderColor: BORDER },
  quickLinkText:  { fontSize: 10, color: "rgba(255,255,255,0.5)", fontFamily: "JetBrainsMono-Regular", textAlign: "center" },

  sectionLabel:   { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.3)", textTransform: "uppercase", letterSpacing: 1, marginBottom: 4, marginTop: 4 },

  faqCard:        { backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 14, padding: 14, gap: 0 },
  faqHeader:      { flexDirection: "row", alignItems: "center", gap: 10 },
  faqIcon:        { width: 32, height: 32, borderRadius: 9, alignItems: "center", justifyContent: "center" },
  faqQ:           { flex: 1, fontSize: 13, color: "#FFF", fontFamily: "SpaceGrotesk-SemiBold", lineHeight: 18 },
  faqBody:        { marginTop: 10, paddingTop: 10, borderTopWidth: 1, borderTopColor: BORDER },
  faqA:           { fontSize: 13, color: "rgba(255,255,255,0.5)", lineHeight: 20 },

  contactRow:     { flexDirection: "row", backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 14, marginTop: 8 },
  contactItem:    { flex: 1, alignItems: "center", gap: 4, padding: 16 },
  contactLabel:   { fontSize: 13, fontFamily: "SpaceGrotesk-SemiBold", color: "#FFF" },
  contactSub:     { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.3)" },
  contactDivider: { width: 1, backgroundColor: BORDER, marginVertical: 12 },

  msgRow:         { flexDirection: "row", alignItems: "flex-end", gap: 8 },
  msgRowUser:     { justifyContent: "flex-end" },
  msgRowAi:       { justifyContent: "flex-start" },
  aiBubbleIcon:   { width: 28, height: 28, borderRadius: 9, backgroundColor: CYAN + "15", alignItems: "center", justifyContent: "center", marginBottom: 2 },
  bubble:         { maxWidth: "78%", borderRadius: 16, paddingHorizontal: 14, paddingVertical: 10 },
  bubbleAi:       { backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderBottomLeftRadius: 4 },
  bubbleUser:     { borderBottomRightRadius: 4 },
  bubbleEscalated:{ borderColor: "rgba(255,171,0,0.35)", backgroundColor: "rgba(255,171,0,0.06)" },
  bubbleOperator: { borderColor: "rgba(0,255,136,0.30)", backgroundColor: "rgba(0,255,136,0.05)" },
  operatorLabel:  { fontSize: 9, fontFamily: "JetBrainsMono-Regular", color: GREEN, letterSpacing: 0.5, marginBottom: 4, textTransform: "uppercase" },
  bubbleText:     { fontSize: 13, color: "rgba(255,255,255,0.8)", lineHeight: 19 },
  bubbleTag:      { flexDirection: "row", alignItems: "center", gap: 4, marginTop: 8, paddingTop: 6, borderTopWidth: 1, borderTopColor: BORDER },
  bubbleTagText:  { fontSize: 9, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.3)", letterSpacing: 0.3 },
  typingDots:     { fontSize: 18, color: CYAN, letterSpacing: 3 },

  promptChips:    { paddingHorizontal: 16, paddingVertical: 8, gap: 8 },
  promptChip:     { paddingHorizontal: 12, paddingVertical: 6, borderRadius: 16, borderWidth: 1, borderColor: BORDER, backgroundColor: GLASS },
  promptChipText: { fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.5)" },

  inputBar:       { flexDirection: "row", gap: 10, paddingHorizontal: 16, paddingVertical: 12, borderTopWidth: 1, borderTopColor: BORDER },
  chatInput:      { flex: 1, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 12, paddingHorizontal: 14, paddingVertical: 10, fontSize: 13, color: "#FFF", fontFamily: "JetBrainsMono-Regular", maxHeight: 80 },
  sendBtn:        { alignSelf: "flex-end" },
  sendBtnGrad:    { width: 42, height: 42, borderRadius: 12, alignItems: "center", justifyContent: "center" },
});

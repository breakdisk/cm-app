/**
 * Customer App — Phone & OTP Screen
 * Step 1 of onboarding: enter mobile number → verify OTP.
 */
import React, { useState, useRef, useEffect } from "react";
import {
  View, Text, StyleSheet, TextInput, Pressable,
  KeyboardAvoidingView, Platform, ScrollView,
} from "react-native";
import Animated, { FadeInDown, FadeInUp } from "react-native-reanimated";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";
import { useDispatch } from "react-redux";
import { authActions } from "../../store";
import type { AppDispatch } from "../../store";

const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const PURPLE = "#A855F7";
const AMBER  = "#FFAB00";
const GLASS  = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

const COUNTRY_CODES = [
  { code: "+971", flag: "🇦🇪", label: "UAE" },
  { code: "+63",  flag: "🇵🇭", label: "PH"  },
  { code: "+966", flag: "🇸🇦", label: "SA"  },
  { code: "+974", flag: "🇶🇦", label: "QA"  },
  { code: "+973", flag: "🇧🇭", label: "BH"  },
  { code: "+968", flag: "🇴🇲", label: "OM"  },
  { code: "+965", flag: "🇰🇼", label: "KW"  },
  { code: "+1",   flag: "🇺🇸", label: "US"  },
  { code: "+44",  flag: "🇬🇧", label: "GB"  },
  { code: "+61",  flag: "🇦🇺", label: "AU"  },
  { code: "+49",  flag: "🇩🇪", label: "DE"  },
];

// Timezone → dial code map for auto-detection
const TZ_TO_DIAL: Record<string, string> = {
  "Asia/Dubai":          "+971",
  "Asia/Muscat":         "+968",
  "Asia/Riyadh":         "+966",
  "Asia/Qatar":          "+974",
  "Asia/Doha":           "+974",
  "Asia/Bahrain":        "+973",
  "Asia/Kuwait":         "+965",
  "Asia/Manila":         "+63",
  "Asia/Cebu":           "+63",
  "America/New_York":    "+1",
  "America/Chicago":     "+1",
  "America/Denver":      "+1",
  "America/Los_Angeles": "+1",
  "America/Toronto":     "+1",
  "America/Vancouver":   "+1",
  "Europe/London":       "+44",
  "Australia/Sydney":    "+61",
  "Australia/Melbourne": "+61",
  "Europe/Berlin":       "+49",
  "Europe/Vienna":       "+49",
};

export function PhoneScreen() {
  const dispatch = useDispatch<AppDispatch>();

  const [stage,        setStage]        = useState<"phone" | "otp">("phone");
  const [countryCode,  setCountryCode]  = useState(COUNTRY_CODES[0]); // default: +971
  const [showPicker,   setShowPicker]   = useState(false);
  const [autoDetected, setAutoDetected] = useState(false);

  // Auto-detect country from device timezone
  useEffect(() => {
    try {
      const tz   = Intl.DateTimeFormat().resolvedOptions().timeZone;
      const dial = TZ_TO_DIAL[tz];
      if (dial) {
        const match = COUNTRY_CODES.find(c => c.code === dial);
        if (match) { setCountryCode(match); setAutoDetected(true); }
      }
    } catch { /* Intl not available — keep default */ }
  }, []);
  const [phone,       setPhone]       = useState("");
  const [otp,         setOtp]         = useState(["", "", "", "", "", ""]);
  const [error,       setError]       = useState("");
  const [sending,     setSending]     = useState(false);

  // Demo mode: fixed OTP code shown on screen so testers can complete the flow
  const DEMO_OTP = "123456";

  const otpRefs = useRef<(TextInput | null)[]>([]);

  function handleSendOtp() {
    if (phone.trim().length < 7) { setError("Enter a valid mobile number"); return; }
    setError("");
    setSending(true);
    // Simulate OTP send — auto-fill demo code after 1 s
    setTimeout(() => {
      setSending(false);
      setOtp(DEMO_OTP.split(""));
      setStage("otp");
    }, 1000);
  }

  function handleOtpChange(val: string, idx: number) {
    if (!/^\d*$/.test(val)) return;
    const next = [...otp];
    next[idx] = val.slice(-1);
    setOtp(next);
    if (val && idx < 5) otpRefs.current[idx + 1]?.focus();
    if (!val && idx > 0) otpRefs.current[idx - 1]?.focus();
  }

  function handleVerify() {
    const code = otp.join("");
    if (code.length < 6) { setError("Enter the 6-digit code"); return; }
    // Simulate verification — accept any 6-digit code
    dispatch(authActions.setPhone(`${countryCode.code}${phone}`));
  }

  const fullPhone = `${countryCode.code} ${phone}`;

  return (
    <KeyboardAvoidingView
      style={{ flex: 1, backgroundColor: CANVAS }}
      behavior={Platform.OS === "ios" ? "padding" : undefined}
    >
      <ScrollView contentContainerStyle={{ flexGrow: 1 }} keyboardShouldPersistTaps="handled">
        <LinearGradient colors={["rgba(0,229,255,0.10)", "transparent"]} style={s.hero}>
          <Animated.View entering={FadeInDown.springify()}>
            <View style={s.logoRow}>
              <View style={s.logoDot} />
              <Text style={s.logoText}>LogisticOS</Text>
            </View>
            <Text style={s.heroTitle}>
              {stage === "phone" ? "Welcome" : "Verify Your Number"}
            </Text>
            <Text style={s.heroSub}>
              {stage === "phone"
                ? "Enter your mobile number to get started"
                : `We sent a 6-digit code to\n${fullPhone}`}
            </Text>
          </Animated.View>
        </LinearGradient>

        <Animated.View entering={FadeInUp.delay(100).springify()} style={s.card}>

          {stage === "phone" ? (
            <>
              <Text style={s.label}>Mobile Number</Text>

              {/* Country code picker trigger */}
              <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
                <Pressable onPress={() => setShowPicker((v) => !v)} style={s.countryTrigger}>
                  <Text style={s.countryFlag}>{countryCode.flag}</Text>
                  <Text style={s.countryCode}>{countryCode.code}</Text>
                  <Ionicons name={showPicker ? "chevron-up" : "chevron-down"} size={12} color="rgba(255,255,255,0.3)" />
                </Pressable>
                {autoDetected && (
                  <View style={s.detectedBadge}>
                    <Ionicons name="locate" size={10} color={GREEN} />
                    <Text style={s.detectedText}>Auto-detected</Text>
                  </View>
                )}
              </View>

              {showPicker && (
                <Animated.View entering={FadeInDown.duration(150)} style={s.countryList}>
                  {COUNTRY_CODES.map((c) => (
                    <Pressable
                      key={c.code}
                      onPress={() => { setCountryCode(c); setShowPicker(false); }}
                      style={[s.countryRow, countryCode.code === c.code && s.countryRowActive]}
                    >
                      <Text style={s.countryFlag}>{c.flag}</Text>
                      <Text style={s.countryCode}>{c.code}</Text>
                      <Text style={s.countryLabel}>{c.label}</Text>
                      {countryCode.code === c.code && <Ionicons name="checkmark" size={14} color={CYAN} />}
                    </Pressable>
                  ))}
                </Animated.View>
              )}

              {/* Phone input */}
              <View style={s.phoneRow}>
                <View style={s.prefixBadge}>
                  <Text style={s.prefixText}>{countryCode.flag} {countryCode.code}</Text>
                </View>
                <TextInput
                  value={phone}
                  onChangeText={(t) => { setPhone(t.replace(/\D/g, "")); setError(""); }}
                  placeholder="9XX XXX XXXX"
                  placeholderTextColor="rgba(255,255,255,0.2)"
                  keyboardType="phone-pad"
                  style={s.phoneInput}
                  maxLength={12}
                />
              </View>

              {error ? <Text style={s.error}>{error}</Text> : null}

              {/* Demo mode hint */}
              <View style={s.demoBox}>
                <Ionicons name="flask-outline" size={13} color={AMBER} />
                <Text style={s.demoText}>Demo mode — OTP will be auto-filled. Any number works.</Text>
              </View>

              <Pressable
                onPress={handleSendOtp}
                disabled={sending || phone.trim().length < 7}
                style={({ pressed }) => [{ opacity: pressed || sending || phone.trim().length < 7 ? 0.5 : 1 }]}
              >
                <LinearGradient colors={[CYAN, PURPLE]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }} style={s.btn}>
                  <Text style={s.btnText}>{sending ? "Sending…" : "Send OTP"}</Text>
                </LinearGradient>
              </Pressable>

              <Text style={s.termsText}>
                By continuing you agree to our Terms of Service and Privacy Policy.
              </Text>
            </>
          ) : (
            <>
              {/* OTP boxes */}
              <Text style={s.label}>Enter 6-digit code</Text>
              <View style={s.otpRow}>
                {otp.map((digit, i) => (
                  <TextInput
                    key={i}
                    ref={(r) => { otpRefs.current[i] = r; }}
                    value={digit}
                    onChangeText={(v) => handleOtpChange(v, i)}
                    keyboardType="number-pad"
                    maxLength={1}
                    style={[s.otpBox, digit ? s.otpBoxFilled : null]}
                    selectTextOnFocus
                  />
                ))}
              </View>

              {error ? <Text style={s.error}>{error}</Text> : null}

              <Pressable
                onPress={handleVerify}
                disabled={otp.join("").length < 6}
                style={({ pressed }) => [{ opacity: pressed || otp.join("").length < 6 ? 0.5 : 1 }]}
              >
                <LinearGradient colors={[GREEN, CYAN]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }} style={s.btn}>
                  <Text style={s.btnText}>Verify & Continue</Text>
                </LinearGradient>
              </Pressable>

              <Pressable onPress={() => { setStage("phone"); setOtp(["","","","","",""]); }} style={s.backLink}>
                <Ionicons name="arrow-back" size={14} color="rgba(255,255,255,0.4)" />
                <Text style={s.backLinkText}>Change number</Text>
              </Pressable>
            </>
          )}
        </Animated.View>
      </ScrollView>
    </KeyboardAvoidingView>
  );
}

const s = StyleSheet.create({
  hero:           { paddingHorizontal: 24, paddingTop: 72, paddingBottom: 24 },
  logoRow:        { flexDirection: "row", alignItems: "center", gap: 8, marginBottom: 24 },
  logoDot:        { width: 10, height: 10, borderRadius: 5, backgroundColor: CYAN, shadowColor: CYAN, shadowOpacity: 0.8, shadowRadius: 6 },
  logoText:       { fontSize: 13, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.4)", letterSpacing: 2, textTransform: "uppercase" },
  heroTitle:      { fontSize: 30, fontFamily: "SpaceGrotesk-Bold", color: "#FFF", marginBottom: 8 },
  heroSub:        { fontSize: 14, color: "rgba(255,255,255,0.4)", lineHeight: 22 },

  card:           { marginHorizontal: 16, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 20, padding: 24, gap: 14 },
  label:          { fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.4)", textTransform: "uppercase", letterSpacing: 1 },

  countryTrigger: { flexDirection: "row", alignItems: "center", gap: 6, alignSelf: "flex-start", backgroundColor: "rgba(255,255,255,0.04)", borderWidth: 1, borderColor: BORDER, borderRadius: 8, paddingHorizontal: 10, paddingVertical: 7 },
  countryFlag:    { fontSize: 16 },
  countryCode:    { fontSize: 13, fontFamily: "JetBrainsMono-Regular", color: "#FFF" },
  countryLabel:   { flex: 1, fontSize: 13, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.5)" },
  countryList:    { backgroundColor: "rgba(5,8,16,0.98)", borderWidth: 1, borderColor: BORDER, borderRadius: 12, overflow: "hidden", marginTop: -6 },
  countryRow:     { flexDirection: "row", alignItems: "center", gap: 10, paddingHorizontal: 14, paddingVertical: 11, borderBottomWidth: 1, borderBottomColor: BORDER },
  countryRowActive: { backgroundColor: "rgba(0,229,255,0.07)" },

  phoneRow:       { flexDirection: "row", gap: 10, alignItems: "center" },
  prefixBadge:    { backgroundColor: "rgba(0,229,255,0.08)", borderWidth: 1, borderColor: "rgba(0,229,255,0.2)", borderRadius: 10, paddingHorizontal: 12, paddingVertical: 12 },
  prefixText:     { fontSize: 13, fontFamily: "JetBrainsMono-Regular", color: CYAN },
  phoneInput:     { flex: 1, backgroundColor: "rgba(255,255,255,0.03)", borderWidth: 1, borderColor: BORDER, borderRadius: 10, paddingHorizontal: 14, paddingVertical: 12, fontSize: 16, color: "#FFF", fontFamily: "JetBrainsMono-Regular", letterSpacing: 2 },

  otpRow:         { flexDirection: "row", gap: 8, justifyContent: "center" },
  otpBox:         { width: 46, height: 54, backgroundColor: "rgba(255,255,255,0.03)", borderWidth: 1, borderColor: BORDER, borderRadius: 12, textAlign: "center", fontSize: 22, color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  otpBoxFilled:   { borderColor: CYAN, backgroundColor: "rgba(0,229,255,0.06)" },

  btn:            { borderRadius: 14, paddingVertical: 15, alignItems: "center" },
  btnText:        { fontSize: 15, fontFamily: "SpaceGrotesk-SemiBold", color: CANVAS },

  error:          { fontSize: 12, color: "#FF3B5C", fontFamily: "JetBrainsMono-Regular", textAlign: "center" },
  termsText:      { fontSize: 11, color: "rgba(255,255,255,0.2)", textAlign: "center", lineHeight: 18 },
  backLink:       { flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 6 },
  backLinkText:   { fontSize: 13, color: "rgba(255,255,255,0.4)" },

  detectedBadge:  { flexDirection: "row", alignItems: "center", gap: 4, backgroundColor: "rgba(0,255,136,0.08)", borderWidth: 1, borderColor: "rgba(0,255,136,0.2)", borderRadius: 6, paddingHorizontal: 7, paddingVertical: 4 },
  detectedText:   { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: GREEN },

  demoBox:        { flexDirection: "row", alignItems: "center", gap: 8, backgroundColor: "rgba(255,171,0,0.07)", borderWidth: 1, borderColor: "rgba(255,171,0,0.25)", borderRadius: 10, paddingHorizontal: 12, paddingVertical: 9 },
  demoText:       { flex: 1, fontSize: 11, color: "rgba(255,171,0,0.8)", fontFamily: "JetBrainsMono-Regular", lineHeight: 17 },
});

/**
 * Sign in — phone, then code.
 *
 * There is no separate sign-up: identity auto-registers on first verify, so a
 * new customer and a returning one take the same path. Adding a registration
 * screen would be a second door to the same room.
 */
import { useCallback, useState } from "react";
import {
  ActivityIndicator,
  Pressable,
  Text,
  TextInput,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useRouter } from "expo-router";

import { normalisePhone, requestOtp, verifyOtp } from "@/api/auth";
import { theme } from "@/theme";

type Step = "phone" | "code";

export default function SignIn() {
  const router = useRouter();
  const [step, setStep] = useState<Step>("phone");
  const [phone, setPhone] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const send = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      await requestOtp(normalisePhone(phone));
      setStep("code");
    } catch (e) {
      setError(
        e instanceof Error
          ? // A fetch that never reached the server throws rather than
            // returning a status — name that, so it is not mistaken for a
            // rejected number.
            (/network|fetch/i.test(e.message) ? `Could not reach the server. ${e.message}` : e.message)
          : "Could not send a code.",
      );
    } finally {
      setBusy(false);
    }
  }, [phone]);

  const verify = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      await verifyOtp(normalisePhone(phone), code);
      // replace, not push: signing in must not leave a back route to itself.
      router.replace("/");
    } catch (e) {
      setError(
        e instanceof Error
          ? (/network|fetch/i.test(e.message) ? `Could not reach the server. ${e.message}` : e.message)
          : "Could not sign you in.",
      );
    } finally {
      setBusy(false);
    }
  }, [phone, code, router]);

  const canSend = phone.replace(/\D/g, "").length >= 10 && !busy;
  const canVerify = code.length === 6 && !busy;

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas }}>
      <View style={{ flex: 1, padding: 24, gap: 18, justifyContent: "center" }}>
        <View style={{ gap: 6 }}>
          <Text style={{ color: theme.text, fontSize: 28, fontWeight: "800" }}>
            {step === "phone" ? "What's your number?" : "Check your messages"}
          </Text>
          <Text style={{ color: theme.muted, fontSize: 14, lineHeight: 20 }}>
            {step === "phone"
              ? "We'll text you a code. Your courier may use this number to reach you."
              : `We sent a 6-digit code to ${normalisePhone(phone)}.`}
          </Text>
        </View>

        {step === "phone" ? (
          <TextInput
            value={phone}
            onChangeText={setPhone}
            placeholder="0917 123 4567"
            placeholderTextColor={theme.faint}
            keyboardType="phone-pad"
            autoComplete="tel"
            autoFocus
            style={inputStyle}
          />
        ) : (
          <TextInput
            value={code}
            onChangeText={(t) => setCode(t.replace(/\D/g, "").slice(0, 6))}
            placeholder="123456"
            placeholderTextColor={theme.faint}
            keyboardType="number-pad"
            autoComplete="sms-otp"
            textContentType="oneTimeCode"
            autoFocus
            style={{ ...inputStyle, letterSpacing: 8, textAlign: "center" }}
          />
        )}

        {error && (
          <Text accessibilityRole="alert" style={{ color: theme.red, fontSize: 13 }}>
            {error}
          </Text>
        )}

        <Pressable
          onPress={step === "phone" ? send : verify}
          disabled={step === "phone" ? !canSend : !canVerify}
          style={{
            backgroundColor: theme.cyan,
            opacity: (step === "phone" ? canSend : canVerify) ? 1 : 0.4,
            borderRadius: theme.radius.md,
            paddingVertical: 15,
            alignItems: "center",
          }}
        >
          {busy ? (
            <ActivityIndicator color="#000" />
          ) : (
            <Text style={{ color: "#000", fontWeight: "700", fontSize: 15 }}>
              {step === "phone" ? "Send code" : "Sign in"}
            </Text>
          )}
        </Pressable>

        {step === "code" && (
          <Pressable
            onPress={() => {
              setStep("phone");
              setCode("");
              setError(null);
            }}
            disabled={busy}
          >
            <Text style={{ color: theme.muted, fontSize: 13, textAlign: "center" }}>
              Wrong number? Go back
            </Text>
          </Pressable>
        )}
      </View>
    </SafeAreaView>
  );
}

const inputStyle = {
  backgroundColor: theme.surface,
  borderColor: theme.border,
  borderWidth: 1,
  borderRadius: theme.radius.md,
  paddingHorizontal: 16,
  paddingVertical: 14,
  color: theme.text,
  fontSize: 18,
} as const;

/**
 * Where should this go?
 *
 * Two ways to answer, because both fail in different situations: the device's
 * location (fast, but denied or wrong indoors) and typed coordinates (always
 * possible, tedious). There is no geocoder wired yet — turning "12 Mabini St"
 * into a point needs the Mapbox token the platform already uses for the driver
 * app, and until that is wired, asking for a street name we cannot resolve
 * would be a form that pretends to work.
 *
 * So: use my location, or enter a point. Honest about what it can do.
 */
import { useCallback, useState } from "react";
import { ActivityIndicator, Pressable, Text, TextInput, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useRouter } from "expo-router";
import * as Location from "expo-location";

import { currentDeliveryPoint, hasDeliveryPoint, saveDeliveryPoint } from "@/deliveryPoint";
import { theme } from "@/theme";

export default function Address() {
  const router = useRouter();
  const existing = hasDeliveryPoint() ? currentDeliveryPoint() : null;

  const [label, setLabel] = useState(existing?.label ?? "");
  const [lat, setLat] = useState(existing ? String(existing.lat) : "");
  const [lng, setLng] = useState(existing ? String(existing.lng) : "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Not a hook. Naming it `useMyLocation` made rules-of-hooks treat the
  // onPress call as a hook invoked inside a callback, which is an error.
  const fillFromDeviceLocation = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const { status } = await Location.requestForegroundPermissionsAsync();
      if (status !== "granted") {
        // Not an error state — a choice. Say what to do instead.
        setError("Location is off. You can enter coordinates below instead.");
        return;
      }
      const pos = await Location.getCurrentPositionAsync({});
      setLat(pos.coords.latitude.toFixed(6));
      setLng(pos.coords.longitude.toFixed(6));
      if (!label) setLabel("Current location");
    } catch {
      setError("Couldn't read your location. Enter coordinates below instead.");
    } finally {
      setBusy(false);
    }
  }, [label]);

  const save = useCallback(async () => {
    const latN = Number(lat);
    const lngN = Number(lng);

    // Validate before saving, not after. A saved nonsense point becomes a
    // courier dispatched to the Gulf of Guinea.
    if (!Number.isFinite(latN) || latN < -90 || latN > 90) {
      setError("That latitude doesn't look right.");
      return;
    }
    if (!Number.isFinite(lngN) || lngN < -180 || lngN > 180) {
      setError("That longitude doesn't look right.");
      return;
    }

    setBusy(true);
    try {
      await saveDeliveryPoint({
        lat: latN,
        lng: lngN,
        label: label.trim() || "My address",
      });
      router.replace("/");
    } catch {
      setError("Couldn't save that address.");
    } finally {
      setBusy(false);
    }
  }, [lat, lng, label, router]);

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas }}>
      <View style={{ flex: 1, padding: 24, gap: 16, justifyContent: "center" }}>
        <View style={{ gap: 6 }}>
          <Text style={{ color: theme.text, fontSize: 26, fontWeight: "800" }}>
            Where should we deliver?
          </Text>
          <Text style={{ color: theme.muted, fontSize: 14, lineHeight: 20 }}>
            We search for shops near here, and this is where your courier goes.
          </Text>
        </View>

        <Pressable
          onPress={() => void fillFromDeviceLocation()}
          disabled={busy}
          style={{
            borderColor: theme.cyan,
            borderWidth: 1,
            borderRadius: theme.radius.md,
            paddingVertical: 13,
            alignItems: "center",
          }}
        >
          <Text style={{ color: theme.cyan, fontWeight: "700", fontSize: 14 }}>
            {busy ? "Locating…" : "Use my location"}
          </Text>
        </Pressable>

        <Text style={{ color: theme.faint, fontSize: 12, textAlign: "center" }}>
          or enter a point
        </Text>

        <TextInput
          value={label}
          onChangeText={setLabel}
          placeholder="Name it — Home, Office…"
          placeholderTextColor={theme.faint}
          style={inputStyle}
        />
        <View style={{ flexDirection: "row", gap: 10 }}>
          <TextInput
            value={lat}
            onChangeText={setLat}
            placeholder="Latitude"
            placeholderTextColor={theme.faint}
            keyboardType="numbers-and-punctuation"
            style={{ ...inputStyle, flex: 1 }}
          />
          <TextInput
            value={lng}
            onChangeText={setLng}
            placeholder="Longitude"
            placeholderTextColor={theme.faint}
            keyboardType="numbers-and-punctuation"
            style={{ ...inputStyle, flex: 1 }}
          />
        </View>

        {error && (
          <Text accessibilityRole="alert" style={{ color: theme.amber, fontSize: 13 }}>
            {error}
          </Text>
        )}

        <Pressable
          onPress={() => void save()}
          disabled={busy || !lat || !lng}
          style={{
            backgroundColor: theme.cyan,
            opacity: busy || !lat || !lng ? 0.4 : 1,
            borderRadius: theme.radius.md,
            paddingVertical: 15,
            alignItems: "center",
          }}
        >
          {busy ? (
            <ActivityIndicator color="#000" />
          ) : (
            <Text style={{ color: "#000", fontWeight: "700", fontSize: 15 }}>
              Save address
            </Text>
          )}
        </Pressable>
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
  paddingVertical: 13,
  color: theme.text,
  fontSize: 15,
} as const;

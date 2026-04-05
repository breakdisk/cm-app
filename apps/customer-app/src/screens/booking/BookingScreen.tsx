/**
 * Customer App — Booking Screen
 * New shipment booking with Local / International (Balikbayan) toggle.
 *
 * Local flow:    address → package details (weight, COD, fragile) → review
 * International: address + country → box dims + declared value + freight mode
 *                → receiver passport upload → review
 */
import React, { useState } from "react";
import {
  View, Text, StyleSheet, ScrollView, Pressable, Image,
  TextInput, Switch, KeyboardAvoidingView, Platform, Alert,
} from "react-native";
import Animated, { FadeInDown, FadeInUp, FadeIn } from "react-native-reanimated";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";
import * as ImagePicker from "expo-image-picker";
import { useDispatch, useSelector } from "react-redux";
import { shipmentsActions, authActions } from "../../store";
import type { AppDispatch, RootState } from "../../store";
import { AwbQRCode } from "../../components/AwbQRCode";
import Toast from "../../components/Toast";
import * as shipmentsService from "../../services/api/shipments";
import { getStoredCustomerId } from "../../services/api/auth";

const CANVAS  = "#050810";
const CYAN    = "#00E5FF";
const GREEN   = "#00FF88";
const AMBER   = "#FFAB00";
const PURPLE  = "#A855F7";
const RED     = "#FF3B5C";
const GLASS   = "rgba(255,255,255,0.04)";
const BORDER  = "rgba(255,255,255,0.08)";

type ShipmentMode = "local" | "international";
type FreightMode  = "sea" | "air";

const SEA_DAYS = "30–45 days";
const AIR_DAYS = "5–10 days";

const BASE_RATE: Record<string, number> = {
  local:         85,
  balikbayan:   500,
};

interface Country { code: string; label: string; flag: string; }

const POPULAR_COUNTRIES: Country[] = [
  { code: "PH", label: "Philippines",    flag: "🇵🇭" },
  { code: "US", label: "United States",  flag: "🇺🇸" },
  { code: "CA", label: "Canada",         flag: "🇨🇦" },
  { code: "GB", label: "United Kingdom", flag: "🇬🇧" },
  { code: "IN", label: "India",          flag: "🇮🇳" },
  { code: "SA", label: "Saudi Arabia",   flag: "🇸🇦" },
  { code: "QA", label: "Qatar",          flag: "🇶🇦" },
  { code: "OM", label: "Oman",           flag: "🇴🇲" },
  { code: "KW", label: "Kuwait",         flag: "🇰🇼" },
  { code: "BH", label: "Bahrain",        flag: "🇧🇭" },
];

const ALL_COUNTRIES: Country[] = [
  { code: "AE", label: "United Arab Emirates", flag: "🇦🇪" },
  { code: "AU", label: "Australia",            flag: "🇦🇺" },
  { code: "AT", label: "Austria",              flag: "🇦🇹" },
  { code: "BE", label: "Belgium",              flag: "🇧🇪" },
  { code: "BR", label: "Brazil",               flag: "🇧🇷" },
  { code: "CN", label: "China",                flag: "🇨🇳" },
  { code: "DK", label: "Denmark",              flag: "🇩🇰" },
  { code: "EG", label: "Egypt",                flag: "🇪🇬" },
  { code: "FR", label: "France",               flag: "🇫🇷" },
  { code: "DE", label: "Germany",              flag: "🇩🇪" },
  { code: "GR", label: "Greece",               flag: "🇬🇷" },
  { code: "HK", label: "Hong Kong",            flag: "🇭🇰" },
  { code: "ID", label: "Indonesia",            flag: "🇮🇩" },
  { code: "IE", label: "Ireland",              flag: "🇮🇪" },
  { code: "IT", label: "Italy",                flag: "🇮🇹" },
  { code: "JP", label: "Japan",                flag: "🇯🇵" },
  { code: "JO", label: "Jordan",               flag: "🇯🇴" },
  { code: "KR", label: "South Korea",          flag: "🇰🇷" },
  { code: "LB", label: "Lebanon",              flag: "🇱🇧" },
  { code: "MY", label: "Malaysia",             flag: "🇲🇾" },
  { code: "MX", label: "Mexico",               flag: "🇲🇽" },
  { code: "NL", label: "Netherlands",          flag: "🇳🇱" },
  { code: "NZ", label: "New Zealand",          flag: "🇳🇿" },
  { code: "NO", label: "Norway",               flag: "🇳🇴" },
  { code: "PK", label: "Pakistan",             flag: "🇵🇰" },
  { code: "PT", label: "Portugal",             flag: "🇵🇹" },
  { code: "SG", label: "Singapore",            flag: "🇸🇬" },
  { code: "ZA", label: "South Africa",         flag: "🇿🇦" },
  { code: "ES", label: "Spain",                flag: "🇪🇸" },
  { code: "SE", label: "Sweden",               flag: "🇸🇪" },
  { code: "CH", label: "Switzerland",          flag: "🇨🇭" },
  { code: "TW", label: "Taiwan",               flag: "🇹🇼" },
  { code: "TH", label: "Thailand",             flag: "🇹🇭" },
  { code: "TR", label: "Turkey",               flag: "🇹🇷" },
  { code: "VN", label: "Vietnam",              flag: "🇻🇳" },
].sort((a, b) => a.label.localeCompare(b.label));

const DEST_COUNTRIES: Country[] = [...POPULAR_COUNTRIES, ...ALL_COUNTRIES];

// ── Searchable Country Picker (React Native) ──────────────────────────────────
function CountryPickerRN({
  value, onChange,
}: { value: string; onChange: (code: string) => void }) {
  const [open,   setOpen]   = React.useState(false);
  const [search, setSearch] = React.useState("");

  const selected = DEST_COUNTRIES.find(c => c.code === value);
  const q = search.toLowerCase();
  const filtPop  = POPULAR_COUNTRIES.filter(c => !q || c.label.toLowerCase().includes(q) || c.code.toLowerCase().includes(q));
  const filtRest = ALL_COUNTRIES.filter(c => !q || c.label.toLowerCase().includes(q) || c.code.toLowerCase().includes(q));

  return (
    <View>
      <Pressable
        onPress={() => setOpen(v => !v)}
        style={[s.inputWrap, { alignItems: "center" }]}
      >
        <Text style={{ fontSize: 16 }}>{selected?.flag ?? "🌐"}</Text>
        <Text style={[s.input, { color: "#FFF", flex: 1 }]}>{selected?.label ?? "Select country"}</Text>
        <Text style={{ fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.3)" }}>{selected?.code}</Text>
        <Ionicons name={open ? "chevron-up" : "chevron-down"} size={13} color="rgba(255,255,255,0.3)" />
      </Pressable>
      {open && (
        <Animated.View entering={FadeIn.duration(150)} style={s.countryPickerDrop}>
          {/* Search */}
          <View style={s.countrySearchRow}>
            <Ionicons name="search-outline" size={13} color="rgba(255,255,255,0.3)" />
            <TextInput
              value={search}
              onChangeText={setSearch}
              placeholder="Search country..."
              placeholderTextColor="rgba(255,255,255,0.2)"
              style={s.countrySearchInput}
              autoFocus={false}
            />
            {search.length > 0 && (
              <Pressable onPress={() => setSearch("")}>
                <Ionicons name="close" size={13} color="rgba(255,255,255,0.3)" />
              </Pressable>
            )}
          </View>
          <ScrollView style={{ maxHeight: 220 }} nestedScrollEnabled>
            {/* Popular */}
            {filtPop.length > 0 && (
              <>
                {!search && <Text style={s.countryGroupLabel}>Common Destinations</Text>}
                {filtPop.map(c => (
                  <Pressable key={c.code} onPress={() => { onChange(c.code); setOpen(false); setSearch(""); }}
                    style={[s.countryOption, value === c.code && { backgroundColor: PURPLE + "15" }]}>
                    <Text style={{ fontSize: 15 }}>{c.flag}</Text>
                    <Text style={[s.countryOptionText, value === c.code && { color: PURPLE }]}>{c.label}</Text>
                    <Text style={s.countryCode}>{c.code}</Text>
                    {value === c.code && <Ionicons name="checkmark" size={12} color={PURPLE} />}
                  </Pressable>
                ))}
              </>
            )}
            {/* Divider + All */}
            {!search && filtRest.length > 0 && (
              <>
                <View style={{ height: 1, backgroundColor: BORDER, marginHorizontal: 12, marginVertical: 4 }} />
                <Text style={s.countryGroupLabel}>All Countries</Text>
              </>
            )}
            {filtRest.map(c => (
              <Pressable key={c.code} onPress={() => { onChange(c.code); setOpen(false); setSearch(""); }}
                style={[s.countryOption, value === c.code && { backgroundColor: PURPLE + "15" }]}>
                <Text style={{ fontSize: 15 }}>{c.flag}</Text>
                <Text style={[s.countryOptionText, value === c.code && { color: PURPLE }]}>{c.label}</Text>
                <Text style={s.countryCode}>{c.code}</Text>
                {value === c.code && <Ionicons name="checkmark" size={12} color={PURPLE} />}
              </Pressable>
            ))}
            {filtPop.length === 0 && filtRest.length === 0 && (
              <Text style={{ textAlign: "center", padding: 20, color: "rgba(255,255,255,0.2)", fontSize: 12, fontFamily: "JetBrainsMono-Regular" }}>
                No countries found
              </Text>
            )}
          </ScrollView>
        </Animated.View>
      )}
    </View>
  );
}

export function BookingScreen() {
  const dispatch     = useDispatch<AppDispatch>();
  const customerId   = useSelector((s: RootState) => s.auth.customerId);

  const [mode,        setMode]        = useState<ShipmentMode>("local");
  const [step,        setStep]        = useState(1);
  const [confirmedAwb, setConfirmedAwb] = useState<string | null>(null);

  // Toast & loading states
  const [toastMessage, setToastMessage] = useState("");
  const [toastType, setToastType] = useState<"success" | "error" | "info">("info");
  const [toastVisible, setToastVisible] = useState(false);
  const [isLoading, setIsLoading] = useState(false);

  const showToast = (message: string, type: "success" | "error" | "info") => {
    setToastMessage(message);
    setToastType(type);
    setToastVisible(true);
  };

  // Step 1 — Sender
  const [senderName,    setSenderName]    = useState("");
  const [senderAddress, setSenderAddress] = useState("");
  const [senderCity,    setSenderCity]    = useState("");
  const [senderZip,     setSenderZip]     = useState("");

  // Step 1 — Receiver
  const [receiverName,    setReceiverName]    = useState("");
  const [receiverAddress, setReceiverAddress] = useState("");
  const [receiverCity,    setReceiverCity]    = useState("");
  const [receiverZip,     setReceiverZip]     = useState("");
  const [destCountry,     setDestCountry]     = useState("US");

  // Step 2 — Package (local)
  const [weight,      setWeight]      = useState("");
  const [description, setDescription] = useState("");
  const [isCOD,       setIsCOD]       = useState(false);
  const [codAmount,   setCodAmount]   = useState("");
  const [isFragile,   setIsFragile]   = useState(false);

  // Step 2 — Package (international / balikbayan)
  const [boxLength,       setBoxLength]       = useState("");
  const [boxWidth,        setBoxWidth]        = useState("");
  const [boxHeight,       setBoxHeight]       = useState("");
  const [declaredValue,   setDeclaredValue]   = useState("");
  const [freightMode,     setFreightMode]     = useState<FreightMode>("sea");
  const [contents,        setContents]        = useState("");

  // Step 3 (international only) — Receiver passport
  const [passportUri,     setPassportUri]     = useState<string | null>(null);

  const isIntl   = mode === "international";
  const totalSteps = isIntl ? 4 : 3;  // international has extra passport step

  function switchMode(m: ShipmentMode) {
    setMode(m);
    setStep(1);
  }

  async function pickPassport() {
    if (Platform.OS === "web") {
      setPassportUri("https://via.placeholder.com/400x260/0A0F1E/A855F7?text=Passport+Bio-data+Page");
      return;
    }
    const perm = await ImagePicker.requestMediaLibraryPermissionsAsync();
    if (!perm.granted) { Alert.alert("Permission needed", "Allow photo access to upload the passport."); return; }
    const result = await ImagePicker.launchImageLibraryAsync({ mediaTypes: ImagePicker.MediaTypeOptions.Images, quality: 0.85, allowsEditing: true, aspect: [4, 3] });
    if (!result.canceled) setPassportUri(result.assets[0].uri);
  }

  async function takePassportPhoto() {
    if (Platform.OS === "web") { pickPassport(); return; }
    const perm = await ImagePicker.requestCameraPermissionsAsync();
    if (!perm.granted) { Alert.alert("Permission needed", "Allow camera access to photograph the passport."); return; }
    const result = await ImagePicker.launchCameraAsync({ quality: 0.85, allowsEditing: true, aspect: [4, 3] });
    if (!result.canceled) setPassportUri(result.assets[0].uri);
  }

  function calcTotal(): number {
    const w = parseFloat(weight || "0");
    const base = isIntl ? BASE_RATE.balikbayan : BASE_RATE.local;
    const weightSurcharge = w > 1 ? Math.ceil((w - 1) / 0.5) * 10 : 0;
    const fragileAdd  = isFragile && !isIntl ? 30 : 0;
    const airPremium  = isIntl && freightMode === "air" ? 800 : 0;
    return base + weightSurcharge + fragileAdd + airPremium;
  }

  async function handleBook() {
    setIsLoading(true);
    try {
      const storedCustomerId = await getStoredCustomerId();
      if (!storedCustomerId) {
        showToast("Not authenticated. Please log in again.", "error");
        return;
      }

      const origin = `${senderAddress}, ${senderCity} ${senderZip}`;
      const destination = `${receiverAddress}, ${receiverCity} ${receiverZip}${isIntl ? ` · ${DEST_COUNTRIES.find(c => c.code === destCountry)?.label || destCountry}` : ""}`;

      const response = await shipmentsService.createShipment(storedCustomerId, {
        origin,
        destination,
        recipientName: receiverName,
        recipientPhone: "", // TODO: add phone field to booking form
        recipientEmail: undefined,
        weight: parseFloat(weight) || 1,
        description: isIntl ? (contents || "Balikbayan Box") : (description || "Parcel"),
        cargoType: isIntl ? "mixed" : "general",
        type: isIntl ? "international" : "local",
        serviceType: isIntl ? (freightMode === "sea" ? "sea" : "air") : "standard",
        codAmount: isCOD && !isIntl ? parseInt(codAmount) : undefined,
      });

      // Update Redux store with the API response
      const now = new Date();
      const bookedAt = now.toLocaleDateString("en-PH", { month: "short", day: "numeric", year: "numeric", hour: "2-digit", minute: "2-digit" });
      const eta = isIntl
        ? (freightMode === "sea" ? "30–45 days" : "5–10 days")
        : now.toLocaleDateString("en-PH", { month: "short", day: "numeric", year: "numeric" });

      dispatch(shipmentsActions.addShipment({
        awb: response.awb,
        type: isIntl ? "international" : "local",
        status: "confirmed",
        origin,
        destination,
        destCountry: isIntl ? destCountry : undefined,
        description: isIntl ? (contents || "Balikbayan Box") : (description || "Parcel"),
        weight: weight || undefined,
        isCOD: isCOD && !isIntl,
        codAmount: isCOD && !isIntl ? codAmount : undefined,
        freightMode: isIntl ? freightMode : undefined,
        bookedAt,
        estimatedDelivery: eta,
        totalFee: calcTotal(),
      }));

      dispatch(authActions.addLoyaltyPts(isIntl ? 150 : 50));
      setConfirmedAwb(response.awb);
      showToast("Shipment booked successfully!", "success");
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to book shipment. Please try again.";
      showToast(errorMessage, "error");
      console.error("Booking error:", error);
    } finally {
      setIsLoading(false);
    }
  }

  function handleBookAnother() {
    setConfirmedAwb(null);
    setStep(1);
    setSenderName(""); setSenderAddress(""); setSenderCity(""); setSenderZip("");
    setReceiverName(""); setReceiverAddress(""); setReceiverCity(""); setReceiverZip("");
    setDestCountry("US");
    setWeight(""); setDescription(""); setIsCOD(false); setCodAmount("");
    setIsFragile(false); setBoxLength(""); setBoxWidth(""); setBoxHeight("");
    setDeclaredValue(""); setContents(""); setPassportUri(null);
    setFreightMode("sea");
  }

  const canProceedStep1 =
    senderName.trim() && senderAddress.trim() && senderCity.trim() && senderZip.trim() &&
    receiverName.trim() && receiverAddress.trim() && receiverCity.trim() && receiverZip.trim();
  const canProceedStep2 = isIntl
    ? boxLength.trim() && boxWidth.trim() && boxHeight.trim() && weight.trim() && declaredValue.trim()
    : weight.trim();
  const canProceedStep3 = !!passportUri; // international only

  return (
    <KeyboardAvoidingView
      style={{ flex: 1, backgroundColor: CANVAS }}
      behavior={Platform.OS === "ios" ? "padding" : undefined}
    >
      <ScrollView contentContainerStyle={{ paddingBottom: 40 }}>

        {/* Hero — hidden when confirmed */}
        {!confirmedAwb && <LinearGradient
          colors={isIntl ? ["rgba(168,85,247,0.12)", "transparent"] : ["rgba(0,255,136,0.10)", "transparent"]}
          style={s.hero}
        >
          <Animated.View entering={FadeInDown.springify()}>
            <Text style={s.heroTitle}>New Shipment</Text>
            <Text style={s.heroSub}>Step {step} of {totalSteps}</Text>
          </Animated.View>
        </LinearGradient>}

        {/* Local / International toggle — hidden when confirmed */}
        {!confirmedAwb && <Animated.View entering={FadeInDown.delay(60).springify()} style={s.modeToggleWrap}>
          <View style={s.modeToggle}>
            <Pressable
              onPress={() => switchMode("local")}
              style={[s.modeBtn, mode === "local" && s.modeBtnActiveLocal]}
            >
              <Ionicons name="home-outline" size={14} color={mode === "local" ? GREEN : "rgba(255,255,255,0.35)"} />
              <Text style={[s.modeBtnText, { color: mode === "local" ? GREEN : "rgba(255,255,255,0.35)" }]}>
                Local
              </Text>
            </Pressable>
            <Pressable
              onPress={() => switchMode("international")}
              style={[s.modeBtn, mode === "international" && s.modeBtnActiveIntl]}
            >
              <Ionicons name="globe-outline" size={14} color={mode === "international" ? PURPLE : "rgba(255,255,255,0.35)"} />
              <Text style={[s.modeBtnText, { color: mode === "international" ? PURPLE : "rgba(255,255,255,0.35)" }]}>
                International
              </Text>
              {mode === "international" && (
                <View style={s.balikbayanTag}>
                  <Text style={s.balikbayanTagText}>Balikbayan</Text>
                </View>
              )}
            </Pressable>
          </View>
        </Animated.View>}

        {/* Balikbayan info banner */}
        {!confirmedAwb && isIntl && (
          <Animated.View entering={FadeIn.duration(300)} style={s.balikbayanBanner}>
            <Ionicons name="information-circle-outline" size={16} color={PURPLE} />
            <View style={{ flex: 1 }}>
              <Text style={s.bannerTitle}>Balikbayan Box Shipping</Text>
              <Text style={s.bannerSub}>
                Special overseas freight for large packages. AI selects optimal carrier and freight mode.
              </Text>
            </View>
          </Animated.View>
        )}

        {/* Step indicator */}
        {!confirmedAwb && <View style={s.stepRow}>
          {Array.from({ length: totalSteps }, (_, i) => i + 1).map((n) => (
            <View
              key={n}
              style={[s.stepDot, {
                backgroundColor: n < step
                  ? (isIntl ? PURPLE : GREEN)
                  : n === step
                    ? (isIntl ? PURPLE : CYAN)
                    : BORDER,
              }]}
            />
          ))}
        </View>}

        {/* ── Step 1 — Sender & Receiver Details ──────────────────────── */}
        {!confirmedAwb && step === 1 && (
          <Animated.View entering={FadeInUp.springify()} style={s.card}>

            {/* Sender section */}
            <View style={s.sectionHeader}>
              <Ionicons name="navigate-circle-outline" size={15} color={CYAN} />
              <Text style={[s.sectionHeading, { color: CYAN }]}>Sender Details</Text>
            </View>

            <TextInput
              value={senderName}
              onChangeText={setSenderName}
              placeholder="Sender's Full Name"
              placeholderTextColor="rgba(255,255,255,0.2)"
              style={s.fieldInput}
            />
            <TextInput
              value={senderAddress}
              onChangeText={setSenderAddress}
              placeholder="Street Address"
              placeholderTextColor="rgba(255,255,255,0.2)"
              style={s.fieldInput}
            />
            <View style={s.rowInputs}>
              <TextInput
                value={senderCity}
                onChangeText={setSenderCity}
                placeholder="City"
                placeholderTextColor="rgba(255,255,255,0.2)"
                style={[s.fieldInput, { flex: 1 }]}
              />
              <TextInput
                value={senderZip}
                onChangeText={setSenderZip}
                placeholder="ZIP Code"
                placeholderTextColor="rgba(255,255,255,0.2)"
                style={[s.fieldInput, { flex: 0.7 }]}
                keyboardType="number-pad"
                maxLength={10}
              />
            </View>

            <View style={s.sectionDivider} />

            {/* Receiver section */}
            <View style={s.sectionHeader}>
              <Ionicons name="location-outline" size={15} color={isIntl ? PURPLE : GREEN} />
              <Text style={[s.sectionHeading, { color: isIntl ? PURPLE : GREEN }]}>Receiver Details</Text>
            </View>

            <TextInput
              value={receiverName}
              onChangeText={setReceiverName}
              placeholder="Receiver's Full Name"
              placeholderTextColor="rgba(255,255,255,0.2)"
              style={[s.fieldInput, isIntl && s.fieldInputIntl]}
            />
            <TextInput
              value={receiverAddress}
              onChangeText={setReceiverAddress}
              placeholder="Street Address"
              placeholderTextColor="rgba(255,255,255,0.2)"
              style={[s.fieldInput, isIntl && s.fieldInputIntl]}
            />
            <View style={s.rowInputs}>
              <TextInput
                value={receiverCity}
                onChangeText={setReceiverCity}
                placeholder="City"
                placeholderTextColor="rgba(255,255,255,0.2)"
                style={[s.fieldInput, { flex: 1 }, isIntl && s.fieldInputIntl]}
              />
              <TextInput
                value={receiverZip}
                onChangeText={setReceiverZip}
                placeholder="ZIP Code"
                placeholderTextColor="rgba(255,255,255,0.2)"
                style={[s.fieldInput, { flex: 0.7 }, isIntl && s.fieldInputIntl]}
                keyboardType="number-pad"
                maxLength={10}
              />
            </View>

            {/* Country — international only */}
            {isIntl && (
              <>
                <Text style={s.label}>Destination Country</Text>
                <CountryPickerRN value={destCountry} onChange={setDestCountry} />
              </>
            )}

            <Pressable
              onPress={() => setStep(2)}
              disabled={!canProceedStep1}
              style={({ pressed }) => [s.btn, { opacity: pressed || !canProceedStep1 ? 0.5 : 1 }]}
            >
              <LinearGradient
                colors={isIntl ? [PURPLE, "#6B21D8"] : [CYAN, PURPLE]}
                start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }}
                style={s.btnGradient}
              >
                <Text style={s.btnText}>Next →</Text>
              </LinearGradient>
            </Pressable>
          </Animated.View>
        )}

        {/* ── Step 2A — Local Package Details ─────────────────────────────── */}
        {!confirmedAwb && step === 2 && !isIntl && (
          <Animated.View entering={FadeInUp.springify()} style={s.card}>
            <Text style={s.cardTitle}>Package Details</Text>

            <Text style={s.label}>Weight (kg)</Text>
            <View style={s.inputWrap}>
              <Ionicons name="scale-outline" size={16} color="rgba(255,255,255,0.3)" />
              <TextInput value={weight} onChangeText={setWeight} placeholder="e.g. 1.5" placeholderTextColor="rgba(255,255,255,0.2)" style={s.input} keyboardType="decimal-pad" />
            </View>

            <Text style={s.label}>Package Description</Text>
            <View style={s.inputWrap}>
              <Ionicons name="cube-outline" size={16} color="rgba(255,255,255,0.3)" />
              <TextInput value={description} onChangeText={setDescription} placeholder="e.g. Electronics, Clothes" placeholderTextColor="rgba(255,255,255,0.2)" style={s.input} />
            </View>

            <View style={s.toggleRow}>
              <View style={{ flex: 1 }}>
                <Text style={s.toggleLabel}>Cash on Delivery (COD)</Text>
                <Text style={s.toggleSub}>Recipient pays on delivery</Text>
              </View>
              <Switch value={isCOD} onValueChange={setIsCOD} trackColor={{ false: BORDER, true: AMBER + "60" }} thumbColor={isCOD ? AMBER : "rgba(255,255,255,0.3)"} />
            </View>
            {isCOD && (
              <View style={[s.inputWrap, { marginTop: 0 }]}>
                <Text style={[s.input, { color: AMBER, flexGrow: 0 }]}>₱</Text>
                <TextInput value={codAmount} onChangeText={setCodAmount} placeholder="0.00" placeholderTextColor="rgba(255,171,0,0.3)" style={[s.input, { color: AMBER }]} keyboardType="decimal-pad" />
              </View>
            )}

            <View style={s.toggleRow}>
              <View style={{ flex: 1 }}>
                <Text style={s.toggleLabel}>Fragile Item</Text>
                <Text style={s.toggleSub}>Handle with care (+₱30)</Text>
              </View>
              <Switch value={isFragile} onValueChange={setIsFragile} trackColor={{ false: BORDER, true: CYAN + "60" }} thumbColor={isFragile ? CYAN : "rgba(255,255,255,0.3)"} />
            </View>

            <View style={{ flexDirection: "row", gap: 10 }}>
              <Pressable onPress={() => setStep(1)} style={({ pressed }) => [s.btn, { flex: 0.4, opacity: pressed ? 0.7 : 1 }]}>
                <View style={[s.btnGradient, { backgroundColor: GLASS }]}>
                  <Text style={[s.btnText, { color: "rgba(255,255,255,0.6)" }]}>← Back</Text>
                </View>
              </Pressable>
              <Pressable
                onPress={() => setStep(3)}
                disabled={!canProceedStep2}
                style={({ pressed }) => [s.btn, { flex: 1, opacity: pressed || !canProceedStep2 ? 0.5 : 1 }]}
              >
                <LinearGradient colors={[CYAN, PURPLE]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }} style={s.btnGradient}>
                  <Text style={s.btnText}>Review →</Text>
                </LinearGradient>
              </Pressable>
            </View>
          </Animated.View>
        )}

        {/* ── Step 2B — International / Balikbayan Box Details ─────────────── */}
        {!confirmedAwb && step === 2 && isIntl && (
          <Animated.View entering={FadeInUp.springify()} style={s.card}>
            <Text style={s.cardTitle}>Box Details</Text>

            {/* Dimensions */}
            <Text style={s.label}>Box Dimensions (cm)</Text>
            <View style={{ flexDirection: "row", gap: 8 }}>
              {[
                { val: boxLength, set: setBoxLength, ph: "Length" },
                { val: boxWidth,  set: setBoxWidth,  ph: "Width"  },
                { val: boxHeight, set: setBoxHeight,  ph: "Height" },
              ].map(({ val, set, ph }) => (
                <View key={ph} style={[s.inputWrap, { flex: 1 }]}>
                  <TextInput
                    value={val}
                    onChangeText={set}
                    placeholder={ph}
                    placeholderTextColor="rgba(255,255,255,0.2)"
                    style={[s.input, { textAlign: "center" }]}
                    keyboardType="decimal-pad"
                  />
                </View>
              ))}
            </View>

            {/* Weight */}
            <Text style={s.label}>Actual Weight (kg)</Text>
            <View style={s.inputWrap}>
              <Ionicons name="scale-outline" size={16} color="rgba(255,255,255,0.3)" />
              <TextInput value={weight} onChangeText={setWeight} placeholder="e.g. 20.5" placeholderTextColor="rgba(255,255,255,0.2)" style={s.input} keyboardType="decimal-pad" />
            </View>

            {/* Contents */}
            <Text style={s.label}>Contents Description</Text>
            <View style={s.inputWrap}>
              <Ionicons name="cube-outline" size={16} color="rgba(255,255,255,0.3)" />
              <TextInput value={contents} onChangeText={setContents} placeholder="e.g. Clothes, canned goods, gadgets" placeholderTextColor="rgba(255,255,255,0.2)" style={s.input} multiline />
            </View>

            {/* Declared value */}
            <Text style={s.label}>Declared Value (PHP) — Required for Customs</Text>
            <View style={s.inputWrap}>
              <Text style={{ fontSize: 14, color: AMBER, fontFamily: "JetBrainsMono-Regular" }}>₱</Text>
              <TextInput value={declaredValue} onChangeText={setDeclaredValue} placeholder="e.g. 15000" placeholderTextColor="rgba(255,171,0,0.3)" style={[s.input, { color: AMBER }]} keyboardType="decimal-pad" />
            </View>
            <Text style={s.fieldNote}>Used for customs declaration. Must reflect actual contents value.</Text>

            {/* Freight mode */}
            <Text style={s.label}>Freight Mode</Text>
            <View style={{ flexDirection: "row", gap: 10 }}>
              {([
                { val: "sea" as FreightMode, icon: "boat-outline",    label: "Sea Freight", sub: SEA_DAYS, color: CYAN,   note: "Most economical" },
                { val: "air" as FreightMode, icon: "airplane-outline", label: "Air Freight", sub: AIR_DAYS, color: AMBER, note: "+₱800 premium" },
              ] as const).map((opt) => (
                <Pressable
                  key={opt.val}
                  onPress={() => setFreightMode(opt.val)}
                  style={[s.freightOption, freightMode === opt.val && { borderColor: opt.color + "60", backgroundColor: opt.color + "0F" }]}
                >
                  <Ionicons name={opt.icon as never} size={20} color={freightMode === opt.val ? opt.color : "rgba(255,255,255,0.35)"} />
                  <Text style={[s.freightLabel, { color: freightMode === opt.val ? opt.color : "rgba(255,255,255,0.7)" }]}>{opt.label}</Text>
                  <Text style={s.freightSub}>{opt.sub}</Text>
                  <Text style={[s.freightNote, { color: freightMode === opt.val ? opt.color + "AA" : "rgba(255,255,255,0.25)" }]}>{opt.note}</Text>
                </Pressable>
              ))}
            </View>

            <View style={{ flexDirection: "row", gap: 10 }}>
              <Pressable onPress={() => setStep(1)} style={({ pressed }) => [s.btn, { flex: 0.4, opacity: pressed ? 0.7 : 1 }]}>
                <View style={[s.btnGradient, { backgroundColor: GLASS }]}>
                  <Text style={[s.btnText, { color: "rgba(255,255,255,0.6)" }]}>← Back</Text>
                </View>
              </Pressable>
              <Pressable
                onPress={() => setStep(3)}
                disabled={!canProceedStep2}
                style={({ pressed }) => [s.btn, { flex: 1, opacity: pressed || !canProceedStep2 ? 0.5 : 1 }]}
              >
                <LinearGradient colors={[PURPLE, "#6B21D8"]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }} style={s.btnGradient}>
                  <Text style={s.btnText}>Review →</Text>
                </LinearGradient>
              </Pressable>
            </View>
          </Animated.View>
        )}

        {/* ── Step 3 (International only) — Receiver Passport ───────────── */}
        {!confirmedAwb && step === 3 && isIntl && (
          <Animated.View entering={FadeInUp.springify()} style={s.card}>
            <Text style={s.cardTitle}>Receiver's Passport</Text>

            <View style={s.passportInfoBox}>
              <Ionicons name="information-circle-outline" size={15} color={PURPLE} />
              <Text style={s.passportInfoText}>
                Required for customs clearance in {DEST_COUNTRIES.find(c => c.code === destCountry)?.label ?? destCountry}. The receiver's passport bio-data page must be valid and clearly photographed.
              </Text>
            </View>

            {/* Requirements */}
            <View style={s.reqBox}>
              {[
                "Receiver's passport — NOT the sender's",
                "Bio-data page (photo + personal details)",
                "Must be valid (not expired)",
                "All text clearly readable, no glare",
              ].map((r, i) => (
                <View key={i} style={s.reqRow}>
                  <Ionicons name="checkmark-circle-outline" size={13} color="rgba(168,85,247,0.6)" />
                  <Text style={s.reqText}>{r}</Text>
                </View>
              ))}
            </View>

            {/* Upload zone / preview */}
            {passportUri ? (
              <Animated.View entering={FadeIn.duration(200)}>
                <Image source={{ uri: passportUri }} style={s.passportPreview} resizeMode="cover" />
                <View style={s.passportPreviewRow}>
                  <Ionicons name="checkmark-circle" size={16} color={GREEN} />
                  <Text style={s.passportPreviewText}>Passport uploaded</Text>
                  <Pressable onPress={() => setPassportUri(null)}>
                    <Text style={s.retakeText}>Retake</Text>
                  </Pressable>
                </View>
              </Animated.View>
            ) : (
              <View style={s.uploadZone}>
                <Ionicons name="document-outline" size={30} color="rgba(168,85,247,0.3)" />
                <Text style={s.uploadTitle}>Upload Passport</Text>
                <Text style={s.uploadSub}>JPG or PNG · Max 10MB</Text>
                <View style={s.uploadBtns}>
                  <Pressable onPress={takePassportPhoto} style={[s.uploadBtn, { borderColor: "rgba(168,85,247,0.3)" }]}>
                    <Ionicons name="camera-outline" size={15} color={PURPLE} />
                    <Text style={[s.uploadBtnText, { color: PURPLE }]}>Camera</Text>
                  </Pressable>
                  <Pressable onPress={pickPassport} style={[s.uploadBtn, { borderColor: "rgba(168,85,247,0.3)" }]}>
                    <Ionicons name="images-outline" size={15} color={PURPLE} />
                    <Text style={[s.uploadBtnText, { color: PURPLE }]}>Gallery</Text>
                  </Pressable>
                </View>
              </View>
            )}

            <View style={{ flexDirection: "row", gap: 10 }}>
              <Pressable onPress={() => setStep(2)} style={({ pressed }) => [s.btn, { flex: 0.4, opacity: pressed ? 0.7 : 1 }]}>
                <View style={[s.btnGradient, { backgroundColor: GLASS }]}>
                  <Text style={[s.btnText, { color: "rgba(255,255,255,0.6)" }]}>← Back</Text>
                </View>
              </Pressable>
              <Pressable
                onPress={() => setStep(4)}
                disabled={!canProceedStep3}
                style={({ pressed }) => [s.btn, { flex: 1, opacity: pressed || !canProceedStep3 ? 0.5 : 1 }]}
              >
                <LinearGradient colors={[PURPLE, "#6B21D8"]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }} style={s.btnGradient}>
                  <Text style={s.btnText}>Review →</Text>
                </LinearGradient>
              </Pressable>
            </View>
          </Animated.View>
        )}

        {/* ── Step 4 (International) / Step 3 (Local) — Review & Confirm ── */}
        {!confirmedAwb && ((isIntl && step === 4) || (!isIntl && step === 3)) && (
          <Animated.View entering={FadeInUp.springify()} style={s.card}>
            <Text style={s.cardTitle}>Review Booking</Text>

            {/* Service type badge */}
            <View style={[s.serviceTypeBadge, isIntl ? s.badgeIntl : s.badgeLocal]}>
              <Ionicons
                name={isIntl ? "globe-outline" : "home-outline"}
                size={13}
                color={isIntl ? PURPLE : GREEN}
              />
              <Text style={[s.serviceTypeText, { color: isIntl ? PURPLE : GREEN }]}>
                {isIntl ? `Balikbayan Box · ${freightMode === "sea" ? "Sea Freight" : "Air Freight"}` : "Local Delivery"}
              </Text>
            </View>

            {/* Sender block */}
            <View style={s.reviewBlock}>
              <Text style={[s.reviewBlockTitle, { color: CYAN }]}>Sender</Text>
              <Text style={s.reviewBlockName}>{senderName}</Text>
              <Text style={s.reviewBlockAddr}>{senderAddress}</Text>
              <Text style={s.reviewBlockAddr}>{senderCity}, {senderZip}</Text>
            </View>

            {/* Receiver block */}
            <View style={[s.reviewBlock, { borderColor: isIntl ? PURPLE + "30" : GREEN + "30" }]}>
              <Text style={[s.reviewBlockTitle, { color: isIntl ? PURPLE : GREEN }]}>Receiver</Text>
              <Text style={s.reviewBlockName}>{receiverName}</Text>
              <Text style={s.reviewBlockAddr}>{receiverAddress}</Text>
              <Text style={s.reviewBlockAddr}>{receiverCity}, {receiverZip}{isIntl ? ` · ${DEST_COUNTRIES.find(c => c.code === destCountry)?.flag ?? ""} ${DEST_COUNTRIES.find(c => c.code === destCountry)?.label ?? destCountry}` : ""}</Text>
            </View>

            {/* Package rows */}
            {!isIntl && [
              { label: "Weight",       value: `${weight} kg` },
              { label: "Description",  value: description || "—" },
              { label: "COD",          value: isCOD ? `₱${codAmount || "0"}` : "No" },
              { label: "Fragile",      value: isFragile ? "Yes (+₱30)" : "No" },
            ].map((r) => (
              <View key={r.label} style={s.reviewRow}>
                <Text style={s.reviewLabel}>{r.label}</Text>
                <Text style={s.reviewValue}>{r.value}</Text>
              </View>
            ))}

            {isIntl && [
              { label: "Dimensions",     value: `${boxLength} × ${boxWidth} × ${boxHeight} cm` },
              { label: "Weight",         value: `${weight} kg` },
              { label: "Contents",       value: contents || "—" },
              { label: "Declared Value", value: `₱${declaredValue}` },
              { label: "Freight Mode",   value: freightMode === "sea" ? `Sea Freight (${SEA_DAYS})` : `Air Freight (${AIR_DAYS})` },
              { label: "Receiver ID",    value: passportUri ? "✓ Passport uploaded" : "—" },
            ].map((r) => (
              <View key={r.label} style={s.reviewRow}>
                <Text style={s.reviewLabel}>{r.label}</Text>
                <Text style={s.reviewValue}>{r.value}</Text>
              </View>
            ))}

            {/* Total */}
            <View style={[s.reviewRow, s.totalRow]}>
              <Text style={s.totalLabel}>Estimated Total</Text>
              <Text style={[s.totalValue, { color: isIntl ? PURPLE : GREEN }]}>
                ₱{calcTotal().toFixed(2)}
              </Text>
            </View>

            {/* Transit note */}
            {isIntl && (
              <View style={s.transitNote}>
                <Ionicons name="time-outline" size={13} color="rgba(168,85,247,0.6)" />
                <Text style={s.transitNoteText}>
                  Estimated transit: {freightMode === "sea" ? SEA_DAYS : AIR_DAYS}. Customs clearance may add 3–7 days.
                </Text>
              </View>
            )}

            <View style={{ flexDirection: "row", gap: 10, marginTop: 4 }}>
              <Pressable onPress={() => setStep(isIntl ? 3 : 2)} style={({ pressed }) => [s.btn, { flex: 0.4, opacity: pressed ? 0.7 : 1 }]}>
                <View style={[s.btnGradient, { backgroundColor: GLASS }]}>
                  <Text style={[s.btnText, { color: "rgba(255,255,255,0.6)" }]}>← Back</Text>
                </View>
              </Pressable>
              <Pressable
                onPress={handleBook}
                disabled={isLoading}
                style={({ pressed }) => [s.btn, { flex: 1, opacity: pressed || isLoading ? 0.5 : 1 }]}
              >
                <LinearGradient
                  colors={isIntl ? [PURPLE, "#6B21D8"] : [GREEN, CYAN]}
                  start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }}
                  style={s.btnGradient}
                >
                  <Text style={s.btnText}>
                    {isLoading ? "Booking..." : (isIntl ? "Book Balikbayan Box" : "Confirm Booking")}
                  </Text>
                </LinearGradient>
              </Pressable>
            </View>
          </Animated.View>
        )}

        {/* ── Booking Confirmed ───────────────────────────────────────── */}
        {confirmedAwb && (
          <Animated.View entering={FadeInUp.springify()} style={s.successCard}>
            <LinearGradient
              colors={isIntl ? ["rgba(168,85,247,0.15)", "rgba(168,85,247,0.04)"] : ["rgba(0,255,136,0.15)", "rgba(0,255,136,0.04)"]}
              style={s.successGradient}
            >
              <View style={[s.successIcon, { backgroundColor: isIntl ? PURPLE + "25" : GREEN + "25" }]}>
                <Ionicons name="checkmark-circle" size={44} color={isIntl ? PURPLE : GREEN} />
              </View>
              <Text style={s.successTitle}>Booking Confirmed!</Text>
              <Text style={s.successSub}>
                {isIntl ? "Your Balikbayan Box has been registered." : "Your shipment has been booked."}
              </Text>

              <View style={s.awbBox}>
                <Text style={s.awbBoxLabel}>Tracking Number</Text>
                <Text style={[s.awbBoxValue, { color: isIntl ? PURPLE : CYAN }]}>{confirmedAwb}</Text>
              </View>

              {/* QR Code for driver pickup scan */}
              <AwbQRCode awb={confirmedAwb} size={180} accent={isIntl ? PURPLE : CYAN} />

              <View style={s.successRows}>
                {[
                  { icon: "time-outline",  label: "Status",      value: "Confirmed" },
                  { icon: "star-outline",  label: "Points Earned", value: `+${isIntl ? 150 : 50} pts` },
                  { icon: "navigate-outline", label: "From",     value: `${senderName} · ${senderCity}` },
                  { icon: "location-outline", label: "To",       value: `${receiverName} · ${receiverCity}${isIntl ? ` (${DEST_COUNTRIES.find(c => c.code === destCountry)?.label ?? destCountry})` : ""}` },
                ].map((r) => (
                  <View key={r.label} style={s.successRow}>
                    <Ionicons name={r.icon as any} size={13} color="rgba(255,255,255,0.3)" />
                    <Text style={s.successRowLabel}>{r.label}</Text>
                    <Text style={[s.successRowValue, r.label === "Points Earned" && { color: isIntl ? PURPLE : GREEN }]}>{r.value}</Text>
                  </View>
                ))}
              </View>

              <Pressable onPress={handleBookAnother} style={[s.btn, { marginTop: 4 }]}>
                <LinearGradient
                  colors={isIntl ? [PURPLE, "#6B21D8"] : [GREEN, CYAN]}
                  start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }}
                  style={s.btnGradient}
                >
                  <Text style={s.btnText}>Book Another Shipment</Text>
                </LinearGradient>
              </Pressable>
            </LinearGradient>
          </Animated.View>
        )}

        <Toast
          message={toastMessage}
          type={toastType}
          visible={toastVisible}
          onHide={() => setToastVisible(false)}
        />
      </ScrollView>
    </KeyboardAvoidingView>
  );
}

const s = StyleSheet.create({
  hero:              { paddingHorizontal: 20, paddingTop: 52, paddingBottom: 16 },
  heroTitle:         { fontSize: 26, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  heroSub:           { fontSize: 13, color: "rgba(255,255,255,0.4)", marginTop: 4 },

  modeToggleWrap:    { paddingHorizontal: 16, marginBottom: 12 },
  modeToggle:        { flexDirection: "row", backgroundColor: "rgba(255,255,255,0.04)", borderWidth: 1, borderColor: BORDER, borderRadius: 14, padding: 4, gap: 4 },
  modeBtn:           { flex: 1, flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 6, paddingVertical: 9, borderRadius: 10 },
  modeBtnActiveLocal:{ backgroundColor: "rgba(0,255,136,0.10)", borderWidth: 1, borderColor: "rgba(0,255,136,0.25)" },
  modeBtnActiveIntl: { backgroundColor: "rgba(168,85,247,0.10)", borderWidth: 1, borderColor: "rgba(168,85,247,0.25)" },
  modeBtnText:       { fontSize: 13, fontFamily: "SpaceGrotesk-SemiBold" },
  balikbayanTag:     { paddingHorizontal: 6, paddingVertical: 2, borderRadius: 6, backgroundColor: "rgba(168,85,247,0.2)" },
  balikbayanTagText: { fontSize: 9, fontFamily: "JetBrainsMono-Regular", color: PURPLE, textTransform: "uppercase" },

  balikbayanBanner:  { marginHorizontal: 16, marginBottom: 10, flexDirection: "row", alignItems: "flex-start", gap: 10, backgroundColor: "rgba(168,85,247,0.07)", borderWidth: 1, borderColor: "rgba(168,85,247,0.2)", borderRadius: 12, padding: 12 },
  bannerTitle:       { fontSize: 12, fontFamily: "SpaceGrotesk-SemiBold", color: PURPLE },
  bannerSub:         { fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: "rgba(168,85,247,0.55)", marginTop: 2 },

  stepRow:           { flexDirection: "row", gap: 6, paddingHorizontal: 16, marginBottom: 16 },
  stepDot:           { flex: 1, height: 3, borderRadius: 2 },

  card:              { marginHorizontal: 16, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 16, padding: 20, gap: 12 },
  cardTitle:         { fontSize: 16, fontWeight: "600", color: "#FFF", fontFamily: "SpaceGrotesk-SemiBold", marginBottom: 4 },

  label:             { fontSize: 11, color: "rgba(255,255,255,0.4)", fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1 },
  inputWrap:         { flexDirection: "row", alignItems: "flex-start", gap: 10, backgroundColor: "rgba(255,255,255,0.03)", borderWidth: 1, borderColor: BORDER, borderRadius: 10, paddingHorizontal: 14, paddingVertical: 12 },
  input:             { flex: 1, fontSize: 14, color: "#FFF", fontFamily: "JetBrainsMono-Regular" },
  fieldNote:         { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.25)", marginTop: -4 },

  toggleRow:         { flexDirection: "row", alignItems: "center", backgroundColor: "rgba(255,255,255,0.03)", borderWidth: 1, borderColor: BORDER, borderRadius: 10, paddingHorizontal: 14, paddingVertical: 12 },
  toggleLabel:       { fontSize: 13, color: "#FFF", fontWeight: "500" },
  toggleSub:         { fontSize: 10, color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular", marginTop: 2 },

  freightOption:     { flex: 1, alignItems: "center", gap: 4, backgroundColor: "rgba(255,255,255,0.03)", borderWidth: 1, borderColor: BORDER, borderRadius: 12, paddingVertical: 14 },
  freightLabel:      { fontSize: 13, fontFamily: "SpaceGrotesk-SemiBold" },
  freightSub:        { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.35)" },
  freightNote:       { fontSize: 9, fontFamily: "JetBrainsMono-Regular" },

  serviceTypeBadge:  { flexDirection: "row", alignItems: "center", gap: 6, alignSelf: "flex-start", paddingHorizontal: 10, paddingVertical: 5, borderRadius: 8, borderWidth: 1, marginBottom: 4 },
  badgeLocal:        { backgroundColor: "rgba(0,255,136,0.08)", borderColor: "rgba(0,255,136,0.25)" },
  badgeIntl:         { backgroundColor: "rgba(168,85,247,0.10)", borderColor: "rgba(168,85,247,0.3)" },
  serviceTypeText:   { fontSize: 12, fontFamily: "SpaceGrotesk-SemiBold" },

  reviewRow:         { flexDirection: "row", justifyContent: "space-between", alignItems: "flex-start", paddingVertical: 8, borderBottomWidth: 1, borderBottomColor: BORDER },
  reviewLabel:       { fontSize: 12, color: "rgba(255,255,255,0.4)", fontFamily: "JetBrainsMono-Regular", flex: 0.45 },
  reviewValue:       { fontSize: 13, color: "#FFF", fontWeight: "500", flex: 0.55, textAlign: "right" },

  totalRow:          { borderBottomWidth: 0, marginTop: 4 },
  totalLabel:        { fontSize: 13, color: "rgba(255,255,255,0.6)", fontFamily: "SpaceGrotesk-SemiBold" },
  totalValue:        { fontSize: 20, fontFamily: "SpaceGrotesk-Bold" },

  transitNote:       { flexDirection: "row", alignItems: "flex-start", gap: 6, backgroundColor: "rgba(168,85,247,0.06)", borderWidth: 1, borderColor: "rgba(168,85,247,0.15)", borderRadius: 8, padding: 10 },
  transitNoteText:   { flex: 1, fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: "rgba(168,85,247,0.6)" },

  passportInfoBox:   { flexDirection: "row", alignItems: "flex-start", gap: 8, backgroundColor: "rgba(168,85,247,0.07)", borderWidth: 1, borderColor: "rgba(168,85,247,0.2)", borderRadius: 10, padding: 11 },
  passportInfoText:  { flex: 1, fontSize: 12, color: "rgba(168,85,247,0.7)", lineHeight: 18 },
  reqBox:            { backgroundColor: "rgba(168,85,247,0.04)", borderWidth: 1, borderColor: "rgba(168,85,247,0.12)", borderRadius: 10, padding: 11, gap: 7 },
  reqRow:            { flexDirection: "row", alignItems: "flex-start", gap: 7 },
  reqText:           { flex: 1, fontSize: 12, color: "rgba(255,255,255,0.45)", lineHeight: 17 },
  uploadZone:        { backgroundColor: "rgba(168,85,247,0.04)", borderWidth: 1, borderColor: "rgba(168,85,247,0.2)", borderRadius: 14, borderStyle: "dashed", paddingVertical: 28, alignItems: "center", gap: 6 },
  uploadTitle:       { fontSize: 13, fontFamily: "SpaceGrotesk-SemiBold", color: "rgba(255,255,255,0.5)" },
  uploadSub:         { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.2)" },
  uploadBtns:        { flexDirection: "row", gap: 10, marginTop: 6 },
  uploadBtn:         { flexDirection: "row", alignItems: "center", gap: 6, backgroundColor: "rgba(168,85,247,0.08)", borderWidth: 1, borderRadius: 10, paddingHorizontal: 14, paddingVertical: 9 },
  uploadBtnText:     { fontSize: 13, fontFamily: "SpaceGrotesk-SemiBold", color: CYAN },
  passportPreview:   { width: "100%", height: 170, borderRadius: 12 },
  passportPreviewRow:{ flexDirection: "row", alignItems: "center", gap: 6, marginTop: 8, justifyContent: "center" },
  passportPreviewText:{ fontSize: 12, color: GREEN, fontFamily: "JetBrainsMono-Regular", flex: 1 },
  retakeText:        { fontSize: 12, color: PURPLE, fontFamily: "JetBrainsMono-Regular" },

  // Step 1 — structured address fields
  sectionHeader:     { flexDirection: "row", alignItems: "center", gap: 6, marginBottom: 6 },
  sectionHeading:    { fontSize: 12, fontFamily: "SpaceGrotesk-SemiBold" },
  sectionDivider:    { height: 1, backgroundColor: BORDER, marginVertical: 8 },
  fieldInput:        { backgroundColor: "rgba(255,255,255,0.03)", borderWidth: 1, borderColor: BORDER, borderRadius: 10, paddingHorizontal: 14, paddingVertical: 12, fontSize: 14, color: "#FFF", fontFamily: "JetBrainsMono-Regular", marginBottom: 6 },
  fieldInputIntl:    { borderColor: "rgba(168,85,247,0.2)" },
  rowInputs:         { flexDirection: "row", gap: 8 },

  // Country picker
  countryPickerDrop: { backgroundColor: "rgba(5,8,16,0.98)", borderWidth: 1, borderColor: BORDER, borderRadius: 12, overflow: "hidden", marginTop: 4, zIndex: 100 },
  countrySearchRow:  { flexDirection: "row", alignItems: "center", gap: 8, borderBottomWidth: 1, borderBottomColor: BORDER, paddingHorizontal: 12, paddingVertical: 10 },
  countrySearchInput:{ flex: 1, fontSize: 12, color: "#FFF", fontFamily: "JetBrainsMono-Regular" },
  countryGroupLabel: { fontSize: 9, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.25)", textTransform: "uppercase", letterSpacing: 1, paddingHorizontal: 12, paddingTop: 8, paddingBottom: 4 },
  countryOption:     { flexDirection: "row", alignItems: "center", gap: 10, paddingHorizontal: 12, paddingVertical: 10, borderBottomWidth: 1, borderBottomColor: "rgba(255,255,255,0.04)" },
  countryOptionText: { flex: 1, fontSize: 12, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.75)" },
  countryCode:       { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.25)" },

  // Review blocks
  reviewBlock:       { backgroundColor: "rgba(255,255,255,0.03)", borderWidth: 1, borderColor: "rgba(0,229,255,0.2)", borderRadius: 12, padding: 12, gap: 3 },
  reviewBlockTitle:  { fontSize: 10, fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1, marginBottom: 2 },
  reviewBlockName:   { fontSize: 14, fontFamily: "SpaceGrotesk-SemiBold", color: "#FFF" },
  reviewBlockAddr:   { fontSize: 12, color: "rgba(255,255,255,0.45)", fontFamily: "JetBrainsMono-Regular" },

  btn:               { borderRadius: 12, overflow: "hidden" },
  btnGradient:       { paddingVertical: 14, alignItems: "center", justifyContent: "center" },
  btnText:           { fontSize: 14, fontWeight: "600", color: CANVAS },

  successCard:        { marginHorizontal: 16, borderRadius: 20, overflow: "hidden", borderWidth: 1, borderColor: BORDER },
  successGradient:    { padding: 24, gap: 14, alignItems: "center" },
  successIcon:        { width: 72, height: 72, borderRadius: 24, alignItems: "center", justifyContent: "center", marginBottom: 4 },
  successTitle:       { fontSize: 22, fontFamily: "SpaceGrotesk-Bold", color: "#FFF" },
  successSub:         { fontSize: 13, color: "rgba(255,255,255,0.45)", textAlign: "center" },
  awbBox:             { alignItems: "center", backgroundColor: "rgba(255,255,255,0.04)", borderWidth: 1, borderColor: BORDER, borderRadius: 12, paddingVertical: 14, paddingHorizontal: 24, width: "100%" },
  awbBoxLabel:        { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.3)", textTransform: "uppercase", letterSpacing: 1, marginBottom: 4 },
  awbBoxValue:        { fontSize: 22, fontFamily: "JetBrainsMono-Regular", fontWeight: "700" },
  successRows:        { width: "100%", gap: 2 },
  successRow:         { flexDirection: "row", alignItems: "center", gap: 8, paddingVertical: 7, borderBottomWidth: 1, borderBottomColor: "rgba(255,255,255,0.05)" },
  successRowLabel:    { flex: 1, fontSize: 12, color: "rgba(255,255,255,0.35)", fontFamily: "JetBrainsMono-Regular" },
  successRowValue:    { fontSize: 13, color: "#FFF", fontFamily: "SpaceGrotesk-SemiBold", textAlign: "right", flex: 1.2 },
});

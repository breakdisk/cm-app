/**
 * Customer App — Booking Screen
 *
 * Simplified logic:
 *   - No manual Local/International toggle
 *   - Sender + Receiver both have: Name, Phone, Address, City, ZIP, Country
 *   - Sender has "Use My Location" → GPS → reverse geocode → fills Address, City, ZIP, Country
 *   - isIntl = senderCountry !== receiverCountry (auto-detected)
 *   - If isIntl → show freight mode (Sea/Air), box dimensions, declared value, passport step
 *   - Fee recalculates whenever either country changes
 *
 * Steps:
 *   Step 1 — Sender details (name, phone, address, city, zip, country, GPS)
 *   Step 2 — Receiver details (name, phone, address, city, zip, country)
 *   Step 3 — Package details (weight, description, COD; or box dims + declared value + freight if intl)
 *   Step 4 — Passport upload (international only)
 *   Step 5 — Review & Confirm
 */
import React, { useState } from "react";
import { useNavigation } from "@react-navigation/native";
import { FadeInView } from '../../components/FadeInView';
import {
  View, Text, StyleSheet, ScrollView, Pressable, Image,
  TextInput, Switch, KeyboardAvoidingView, Platform, Alert,
} from "react-native";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";
import * as ImagePicker from "expo-image-picker";
import * as Location from "expo-location";
import {
  getTier, calcEarnedPoints, maxRedemptionValue,
  ptsForDiscount, applyTierDiscount, ptsToPhp, REDEMPTION_MIN,
} from "../../utils/loyalty";
import { useDispatch, useSelector } from "react-redux";
import { useNetInfo } from "@react-native-community/netinfo";
import { shipmentsActions, authActions } from "../../store";
import type { AppDispatch, RootState } from "../../store";
import { AwbQRCode } from "../../components/AwbQRCode";
import Toast from "../../components/Toast";
import * as shipmentsService from "../../services/api/shipments";
import { getStoredCustomerId } from "../../services/api/auth";
import { savePendingShipment } from "../../db/sync";

const CANVAS  = "#050810";
const CYAN    = "#00E5FF";
const GREEN   = "#00FF88";
const AMBER   = "#FFAB00";
const PURPLE  = "#A855F7";
const GLASS   = "rgba(255,255,255,0.04)";
const BORDER  = "rgba(255,255,255,0.08)";

type FreightMode = "sea" | "air";

const SEA_DAYS = "30–45 days";
const AIR_DAYS = "5–10 days";

const BASE_RATE = { local: 85, international: 500 };

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
  { code: "AE", label: "United Arab Emirates", flag: "🇦🇪" },
];

const ALL_COUNTRIES: Country[] = [
  { code: "AU", label: "Australia",    flag: "🇦🇺" },
  { code: "AT", label: "Austria",      flag: "🇦🇹" },
  { code: "BE", label: "Belgium",      flag: "🇧🇪" },
  { code: "BR", label: "Brazil",       flag: "🇧🇷" },
  { code: "CN", label: "China",        flag: "🇨🇳" },
  { code: "DK", label: "Denmark",      flag: "🇩🇰" },
  { code: "EG", label: "Egypt",        flag: "🇪🇬" },
  { code: "FR", label: "France",       flag: "🇫🇷" },
  { code: "DE", label: "Germany",      flag: "🇩🇪" },
  { code: "GR", label: "Greece",       flag: "🇬🇷" },
  { code: "HK", label: "Hong Kong",    flag: "🇭🇰" },
  { code: "ID", label: "Indonesia",    flag: "🇮🇩" },
  { code: "IE", label: "Ireland",      flag: "🇮🇪" },
  { code: "IT", label: "Italy",        flag: "🇮🇹" },
  { code: "JP", label: "Japan",        flag: "🇯🇵" },
  { code: "JO", label: "Jordan",       flag: "🇯🇴" },
  { code: "KR", label: "South Korea",  flag: "🇰🇷" },
  { code: "LB", label: "Lebanon",      flag: "🇱🇧" },
  { code: "MY", label: "Malaysia",     flag: "🇲🇾" },
  { code: "MX", label: "Mexico",       flag: "🇲🇽" },
  { code: "NL", label: "Netherlands",  flag: "🇳🇱" },
  { code: "NZ", label: "New Zealand",  flag: "🇳🇿" },
  { code: "NO", label: "Norway",       flag: "🇳🇴" },
  { code: "PK", label: "Pakistan",     flag: "🇵🇰" },
  { code: "PT", label: "Portugal",     flag: "🇵🇹" },
  { code: "SG", label: "Singapore",    flag: "🇸🇬" },
  { code: "ZA", label: "South Africa", flag: "🇿🇦" },
  { code: "ES", label: "Spain",        flag: "🇪🇸" },
  { code: "SE", label: "Sweden",       flag: "🇸🇪" },
  { code: "CH", label: "Switzerland",  flag: "🇨🇭" },
  { code: "TW", label: "Taiwan",       flag: "🇹🇼" },
  { code: "TH", label: "Thailand",     flag: "🇹🇭" },
  { code: "TR", label: "Turkey",       flag: "🇹🇷" },
  { code: "VN", label: "Vietnam",      flag: "🇻🇳" },
].sort((a, b) => a.label.localeCompare(b.label));

const ALL_COUNTRY_LIST: Country[] = [...POPULAR_COUNTRIES, ...ALL_COUNTRIES];

// ── Country picker ──────────────────────────────────────────────────────────

function CountryPickerRN({ value, onChange, accent = CYAN }: {
  value: string;
  onChange: (code: string) => void;
  accent?: string;
}) {
  const [open,   setOpen]   = React.useState(false);
  const [search, setSearch] = React.useState("");

  const selected = ALL_COUNTRY_LIST.find(c => c.code === value);
  const q = search.toLowerCase();
  const filtPop  = POPULAR_COUNTRIES.filter(c => !q || c.label.toLowerCase().includes(q) || c.code.toLowerCase().includes(q));
  const filtRest = ALL_COUNTRIES.filter(c => !q || c.label.toLowerCase().includes(q) || c.code.toLowerCase().includes(q));

  return (
    <View>
      <Pressable onPress={() => setOpen(v => !v)} style={[s.inputWrap, { alignItems: "center" }]}>
        <Text style={{ fontSize: 16 }}>{selected?.flag ?? "🌐"}</Text>
        <Text style={[s.input, { color: "#FFF", flex: 1 }]}>{selected?.label ?? "Select country"}</Text>
        <Text style={{ fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.3)" }}>{selected?.code}</Text>
        <Ionicons name={open ? "chevron-up" : "chevron-down"} size={13} color="rgba(255,255,255,0.3)" />
      </Pressable>
      {open && (
        <FadeInView duration={150} style={s.countryPickerDrop}>
          <View style={s.countrySearchRow}>
            <Ionicons name="search-outline" size={13} color="rgba(255,255,255,0.3)" />
            <TextInput
              value={search}
              onChangeText={setSearch}
              placeholder="Search country..."
              placeholderTextColor="rgba(255,255,255,0.2)"
              style={s.countrySearchInput}
            />
            {search.length > 0 && (
              <Pressable onPress={() => setSearch("")}>
                <Ionicons name="close" size={13} color="rgba(255,255,255,0.3)" />
              </Pressable>
            )}
          </View>
          <ScrollView style={{ maxHeight: 220 }} nestedScrollEnabled>
            {filtPop.length > 0 && (
              <>
                {!search && <Text style={s.countryGroupLabel}>Common</Text>}
                {filtPop.map(c => (
                  <Pressable key={c.code} onPress={() => { onChange(c.code); setOpen(false); setSearch(""); }}
                    style={[s.countryOption, value === c.code && { backgroundColor: accent + "15" }]}>
                    <Text style={{ fontSize: 15 }}>{c.flag}</Text>
                    <Text style={[s.countryOptionText, value === c.code && { color: accent }]}>{c.label}</Text>
                    <Text style={s.countryCode}>{c.code}</Text>
                    {value === c.code && <Ionicons name="checkmark" size={12} color={accent} />}
                  </Pressable>
                ))}
              </>
            )}
            {!search && filtRest.length > 0 && (
              <>
                <View style={{ height: 1, backgroundColor: BORDER, marginHorizontal: 12, marginVertical: 4 }} />
                <Text style={s.countryGroupLabel}>All Countries</Text>
              </>
            )}
            {filtRest.map(c => (
              <Pressable key={c.code} onPress={() => { onChange(c.code); setOpen(false); setSearch(""); }}
                style={[s.countryOption, value === c.code && { backgroundColor: accent + "15" }]}>
                <Text style={{ fontSize: 15 }}>{c.flag}</Text>
                <Text style={[s.countryOptionText, value === c.code && { color: accent }]}>{c.label}</Text>
                <Text style={s.countryCode}>{c.code}</Text>
                {value === c.code && <Ionicons name="checkmark" size={12} color={accent} />}
              </Pressable>
            ))}
            {filtPop.length === 0 && filtRest.length === 0 && (
              <Text style={{ textAlign: "center", padding: 20, color: "rgba(255,255,255,0.2)", fontSize: 12 }}>
                No countries found
              </Text>
            )}
          </ScrollView>
        </FadeInView>
      )}
    </View>
  );
}

// ── Main screen ─────────────────────────────────────────────────────────────

export function BookingScreen() {
  const dispatch      = useDispatch<AppDispatch>();
  const navigation    = useNavigation<any>();
  const loyaltyPoints = useSelector((s: RootState) => s.auth.loyaltyPoints);
  const shipmentCount = useSelector((s: RootState) => s.shipments.list.length);
  const { isConnected } = useNetInfo();

  const [step, setStep] = useState(1);
  const [confirmedAwb, setConfirmedAwb] = useState<string | null>(null);

  // Toast & loading
  const [toastMessage, setToastMessage] = useState("");
  const [toastType, setToastType]       = useState<"success" | "error" | "info">("info");
  const [toastVisible, setToastVisible] = useState(false);
  const [isLoading, setIsLoading]       = useState(false);
  const [locLoading, setLocLoading]     = useState(false);

  const showToast = (msg: string, type: "success" | "error" | "info") => {
    setToastMessage(msg); setToastType(type); setToastVisible(true);
  };

  // ── Step 1 — Sender
  const [senderName,    setSenderName]    = useState("");
  const [senderPhone,   setSenderPhone]   = useState("");
  const [senderAddress, setSenderAddress] = useState("");
  const [senderCity,    setSenderCity]    = useState("");
  const [senderZip,     setSenderZip]     = useState("");
  const [senderCountry, setSenderCountry] = useState("PH");
  const [pickupCoords,  setPickupCoords]  = useState<{ lat: number; lng: number } | null>(null);

  // ── Step 2 — Receiver
  const [receiverName,    setReceiverName]    = useState("");
  const [receiverPhone,   setReceiverPhone]   = useState("");
  const [receiverAddress, setReceiverAddress] = useState("");
  const [receiverCity,    setReceiverCity]    = useState("");
  const [receiverZip,     setReceiverZip]     = useState("");
  const [receiverCountry, setReceiverCountry] = useState("PH");

  // ── Step 3 — Package
  const [weight,        setWeight]        = useState("");
  const [description,   setDescription]   = useState("");
  const [isCOD,         setIsCOD]         = useState(false);
  const [codAmount,     setCodAmount]      = useState("");
  const [isFragile,     setIsFragile]      = useState(false);
  // International extras
  const [boxLength,     setBoxLength]     = useState("");
  const [boxWidth,      setBoxWidth]      = useState("");
  const [boxHeight,     setBoxHeight]     = useState("");
  const [declaredValue, setDeclaredValue] = useState("");
  const [freightMode,   setFreightMode]   = useState<FreightMode>("sea");
  const [contents,      setContents]      = useState("");

  // ── Step 4 — Passport (international only)
  const [passportUri, setPassportUri] = useState<string | null>(null);

  // ── Loyalty
  const [redeemPoints, setRedeemPoints] = useState(false);
  const [redeemAmount,  setRedeemAmount]  = useState(0);

  // Auto-detect shipment type
  const isIntl   = senderCountry !== receiverCountry;
  const totalSteps = isIntl ? 5 : 4; // passport step only for international
  const accent   = isIntl ? PURPLE : CYAN;
  const accentAlt = isIntl ? PURPLE : GREEN;

  // ── GPS ──────────────────────────────────────────────────────────────────

  async function useMyLocation() {
    setLocLoading(true);
    try {
      const { status } = await Location.requestForegroundPermissionsAsync();
      if (status !== "granted") {
        Alert.alert("Permission needed", "Allow location access so the driver can find you for pickup.");
        return;
      }
      const pos = await Location.getCurrentPositionAsync({ accuracy: Location.Accuracy.High });
      const { latitude, longitude } = pos.coords;
      setPickupCoords({ lat: latitude, lng: longitude });

      const geo = await Location.reverseGeocodeAsync({ latitude, longitude });
      if (geo.length > 0) {
        const g = geo[0];
        const street = [g.streetNumber, g.street].filter(Boolean).join(" ");
        if (street)        setSenderAddress(street);
        if (g.city)        setSenderCity(g.city);
        if (g.postalCode)  setSenderZip(g.postalCode);
        // Map expo's isoCountryCode to our country code
        if (g.isoCountryCode) setSenderCountry(g.isoCountryCode);
      }
      showToast("📍 Location captured — driver will navigate to your pin", "success");
    } catch {
      Alert.alert("Location error", "Could not get your location. Please enter address manually.");
    } finally {
      setLocLoading(false);
    }
  }

  // ── Passport ─────────────────────────────────────────────────────────────

  async function pickPassport() {
    if (Platform.OS === "web") {
      setPassportUri("https://via.placeholder.com/400x260/0A0F1E/A855F7?text=Passport+Bio-data+Page");
      return;
    }
    const perm = await ImagePicker.requestMediaLibraryPermissionsAsync();
    if (!perm.granted) { Alert.alert("Permission needed", "Allow photo access to upload the passport."); return; }
    const result = await ImagePicker.launchImageLibraryAsync({ mediaTypes: ['images'] as any, quality: 0.85, allowsEditing: true, aspect: [4, 3] });
    if (!result.canceled && result.assets.length > 0) setPassportUri(result.assets[0].uri);
  }

  async function takePassportPhoto() {
    if (Platform.OS === "web") { pickPassport(); return; }
    const perm = await ImagePicker.requestCameraPermissionsAsync();
    if (!perm.granted) { Alert.alert("Permission needed", "Allow camera access to photograph the passport."); return; }
    const result = await ImagePicker.launchCameraAsync({ quality: 0.85, allowsEditing: true, aspect: [4, 3] });
    if (!result.canceled) setPassportUri(result.assets[0].uri);
  }

  // ── Fee calculation ───────────────────────────────────────────────────────

  function calcBaseTotal(): number {
    const w = parseFloat(weight || "0");
    const base = isIntl ? BASE_RATE.international : BASE_RATE.local;
    const weightSurcharge = w > 1 ? Math.ceil((w - 1) / 0.5) * 10 : 0;
    const fragileAdd = isFragile && !isIntl ? 30 : 0;
    const airPremium = isIntl && freightMode === "air" ? 800 : 0;
    return base + weightSurcharge + fragileAdd + airPremium;
  }

  function calcTotal(): number {
    const base = calcBaseTotal();
    const afterTier = applyTierDiscount(base, loyaltyPoints);
    return Math.max(0, afterTier - (redeemPoints ? redeemAmount : 0));
  }

  function calcPointsToEarn(): number {
    return calcEarnedPoints({
      type: isIntl ? "international" : "local",
      isCOD: isCOD && !isIntl,
      isFirstBooking: shipmentCount === 0,
      currentPts: loyaltyPoints,
    });
  }

  // ── Booking submission ────────────────────────────────────────────────────

  async function handleBook() {
    setIsLoading(true);
    try {
      const storedCustomerId = await getStoredCustomerId();
      if (!storedCustomerId) { showToast("Not authenticated. Please log in again.", "error"); return; }

      const senderCountryLabel = ALL_COUNTRY_LIST.find(c => c.code === senderCountry)?.label ?? senderCountry;
      const receiverCountryLabel = ALL_COUNTRY_LIST.find(c => c.code === receiverCountry)?.label ?? receiverCountry;

      const origin = `${senderAddress}, ${senderCity} ${senderZip}, ${senderCountryLabel}${pickupCoords ? ` [GPS:${pickupCoords.lat.toFixed(6)},${pickupCoords.lng.toFixed(6)}]` : ""}`;
      const destination = `${receiverAddress}, ${receiverCity} ${receiverZip}, ${receiverCountryLabel}`;

      if (!isConnected) {
        await savePendingShipment(storedCustomerId, {
          origin, destination,
          recipientName: receiverName,
          recipientPhone: receiverPhone,
          weight: parseFloat(weight) || 1,
          type: isIntl ? "international" : "local",
          fee: calcTotal(),
          currency: "PHP",
          codAmount: isCOD && !isIntl ? parseInt(codAmount) : undefined,
        });
        const tempAwb = `OFFLINE-${Math.random().toString(36).substring(2, 10).toUpperCase()}`;
        setConfirmedAwb(tempAwb);
        showToast("Saved offline. Will sync when online.", "info");
        return;
      }

      const response = await shipmentsService.createShipment({
        customer_name:  receiverName,
        customer_phone: receiverPhone,
        origin: {
          line1:        senderAddress,
          city:         senderCity,
          province:     senderCity,
          postal_code:  senderZip || '0000',
          country_code: senderCountry || 'PH',
        },
        destination: {
          line1:        receiverAddress,
          city:         receiverCity,
          province:     receiverCity,
          postal_code:  receiverZip || '0000',
          country_code: receiverCountry || 'PH',
        },
        service_type:      isIntl ? (freightMode === 'sea' ? 'balikbayan' : 'standard') : 'standard',
        weight_grams:      Math.round((parseFloat(weight) || 1) * 1000),
        description:       isIntl ? (contents || 'Balikbayan Box') : (description || 'Parcel'),
        cod_amount_cents:  isCOD && !isIntl ? Math.round(parseInt(codAmount || '0') * 100) : undefined,
        declared_value_cents: Math.round(calcTotal() * 100),
      });

      const now = new Date();
      const bookedAt = now.toLocaleDateString("en-PH", { month: "short", day: "numeric", year: "numeric", hour: "2-digit", minute: "2-digit" });
      const eta = isIntl
        ? (freightMode === "sea" ? "30–45 days" : "5–10 days")
        : now.toLocaleDateString("en-PH", { month: "short", day: "numeric", year: "numeric" });

      dispatch(shipmentsActions.addShipment({
        awb: response.awb ?? response.tracking_number,
        type: isIntl ? "international" : "local",
        status: "confirmed",
        origin, destination,
        destCountry: isIntl ? receiverCountry : undefined,
        description: isIntl ? (contents || "Balikbayan Box") : (description || "Parcel"),
        weight: weight || undefined,
        isCOD: isCOD && !isIntl,
        codAmount: isCOD && !isIntl ? codAmount : undefined,
        freightMode: isIntl ? freightMode : undefined,
        bookedAt, estimatedDelivery: eta,
        totalFee: calcTotal(),
      }));

      const earned = calcPointsToEarn();
      dispatch(authActions.addLoyaltyPts(earned));
      if (redeemPoints && redeemAmount > 0) {
        dispatch(authActions.addLoyaltyPts(-ptsForDiscount(redeemAmount)));
      }

      setConfirmedAwb(response.awb);
      showToast(`Booked! +${earned} loyalty points earned 🎉`, "success");
    } catch (error) {
      showToast(error instanceof Error ? error.message : "Failed to book shipment.", "error");
    } finally {
      setIsLoading(false);
    }
  }

  function handleBookAnother() {
    setConfirmedAwb(null); setStep(1);
    setSenderName(""); setSenderPhone(""); setSenderAddress(""); setSenderCity(""); setSenderZip(""); setSenderCountry("PH"); setPickupCoords(null);
    setReceiverName(""); setReceiverPhone(""); setReceiverAddress(""); setReceiverCity(""); setReceiverZip(""); setReceiverCountry("PH");
    setWeight(""); setDescription(""); setIsCOD(false); setCodAmount(""); setIsFragile(false);
    setBoxLength(""); setBoxWidth(""); setBoxHeight(""); setDeclaredValue(""); setContents(""); setFreightMode("sea");
    setPassportUri(null); setRedeemPoints(false); setRedeemAmount(0);
  }

  // ── Validation per step ───────────────────────────────────────────────────

  const canStep1 = senderName.trim() && senderPhone.trim() && senderAddress.trim() && senderCity.trim() && senderZip.trim();
  const canStep2 = receiverName.trim() && receiverPhone.trim() && receiverAddress.trim() && receiverCity.trim() && receiverZip.trim();
  const canStep3 = isIntl
    ? boxLength.trim() && boxWidth.trim() && boxHeight.trim() && weight.trim() && declaredValue.trim()
    : weight.trim();
  const canStep4 = !!passportUri; // international passport step

  // Step numbers for review
  const reviewStep = isIntl ? 5 : 4;
  const passportStep = 4; // only reached when isIntl

  // ── Helpers ───────────────────────────────────────────────────────────────

  const senderCountryInfo   = ALL_COUNTRY_LIST.find(c => c.code === senderCountry);
  const receiverCountryInfo = ALL_COUNTRY_LIST.find(c => c.code === receiverCountry);

  function BackBtn({ to }: { to: number }) {
    return (
      <Pressable onPress={() => setStep(to)} style={({ pressed }) => [s.btn, { flex: 0.4, opacity: pressed ? 0.7 : 1 }]}>
        <View style={[s.btnGradient, { backgroundColor: GLASS }]}>
          <Text style={[s.btnText, { color: "rgba(255,255,255,0.6)" }]}>← Back</Text>
        </View>
      </Pressable>
    );
  }

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <KeyboardAvoidingView style={{ flex: 1, backgroundColor: CANVAS }} behavior={Platform.OS === "ios" ? "padding" : undefined}>
      <ScrollView contentContainerStyle={{ paddingBottom: 40 }}>

        {/* Hero */}
        {!confirmedAwb && (
          <LinearGradient
            colors={isIntl ? ["rgba(168,85,247,0.12)", "transparent"] : ["rgba(0,229,255,0.10)", "transparent"]}
            style={s.hero}
          >
            <FadeInView fromY={-12}>
              <Text style={s.heroTitle}>New Shipment</Text>
              <Text style={s.heroSub}>Step {step} of {totalSteps}</Text>
            </FadeInView>
          </LinearGradient>
        )}

        {/* Auto-detected type badge */}
        {!confirmedAwb && (
          <FadeInView delay={40} fromY={-8} style={s.typeBadgeWrap}>
            <View style={[s.typeBadge, isIntl ? s.typeBadgeIntl : s.typeBadgeLocal]}>
              <Ionicons name={isIntl ? "globe-outline" : "home-outline"} size={13} color={isIntl ? PURPLE : GREEN} />
              <Text style={[s.typeBadgeText, { color: isIntl ? PURPLE : GREEN }]}>
                {isIntl ? `International · ${senderCountryInfo?.flag ?? ""} → ${receiverCountryInfo?.flag ?? ""}` : "Local Delivery"}
              </Text>
            </View>
          </FadeInView>
        )}

        {/* Step progress dots */}
        {!confirmedAwb && (
          <View style={s.stepRow}>
            {Array.from({ length: totalSteps }, (_, i) => i + 1).map(n => (
              <View key={n} style={[s.stepDot, {
                backgroundColor: n < step ? accentAlt : n === step ? accent : BORDER,
              }]} />
            ))}
          </View>
        )}

        {/* ── STEP 1 — Sender ──────────────────────────────────────────── */}
        {!confirmedAwb && step === 1 && (
          <FadeInView fromY={16} style={s.card}>

            <View style={s.sectionHeader}>
              <Ionicons name="navigate-circle-outline" size={15} color={CYAN} />
              <Text style={[s.sectionHeading, { color: CYAN }]}>Sender / Pickup</Text>
            </View>

            {/* GPS button */}
            <Pressable onPress={useMyLocation} disabled={locLoading} style={[s.gpsBtn, pickupCoords && s.gpsBtnActive]}>
              <Ionicons name={pickupCoords ? "location" : "locate-outline"} size={16} color={pickupCoords ? GREEN : CYAN} />
              <Text style={[s.gpsBtnText, pickupCoords && { color: GREEN }]}>
                {locLoading ? "Getting location…" : pickupCoords ? `📍 GPS pin saved (${pickupCoords.lat.toFixed(4)}, ${pickupCoords.lng.toFixed(4)})` : "Use My Current Location"}
              </Text>
              {pickupCoords && (
                <Pressable onPress={() => setPickupCoords(null)} hitSlop={8}>
                  <Ionicons name="close-circle" size={15} color="rgba(255,255,255,0.3)" />
                </Pressable>
              )}
            </Pressable>

            <TextInput value={senderName} onChangeText={setSenderName}
              placeholder="Sender's Full Name *" placeholderTextColor="rgba(255,255,255,0.2)" style={s.fieldInput} />

            <TextInput value={senderPhone} onChangeText={setSenderPhone}
              placeholder="Sender's Phone Number *" placeholderTextColor="rgba(255,255,255,0.2)"
              style={s.fieldInput} keyboardType="phone-pad" />

            <TextInput value={senderAddress} onChangeText={setSenderAddress}
              placeholder="Street Address *" placeholderTextColor="rgba(255,255,255,0.2)" style={s.fieldInput} />

            <View style={s.rowInputs}>
              <TextInput value={senderCity} onChangeText={setSenderCity}
                placeholder="City *" placeholderTextColor="rgba(255,255,255,0.2)"
                style={[s.fieldInput, { flex: 1 }]} />
              <TextInput value={senderZip} onChangeText={setSenderZip}
                placeholder="ZIP *" placeholderTextColor="rgba(255,255,255,0.2)"
                style={[s.fieldInput, { flex: 0.7 }]} keyboardType="number-pad" maxLength={10} />
            </View>

            <Text style={s.label}>Country</Text>
            <CountryPickerRN value={senderCountry} onChange={setSenderCountry} accent={CYAN} />

            <Pressable onPress={() => setStep(2)} disabled={!canStep1}
              style={({ pressed }) => [s.btn, { opacity: pressed || !canStep1 ? 0.5 : 1, marginTop: 8 }]}>
              <LinearGradient colors={[CYAN, PURPLE]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }} style={s.btnGradient}>
                <Text style={s.btnText}>Next →</Text>
              </LinearGradient>
            </Pressable>
          </FadeInView>
        )}

        {/* ── STEP 2 — Receiver ────────────────────────────────────────── */}
        {!confirmedAwb && step === 2 && (
          <FadeInView fromY={16} style={s.card}>

            <View style={s.sectionHeader}>
              <Ionicons name="location-outline" size={15} color={accentAlt} />
              <Text style={[s.sectionHeading, { color: accentAlt }]}>Receiver / Delivery</Text>
            </View>

            <TextInput value={receiverName} onChangeText={setReceiverName}
              placeholder="Receiver's Full Name *" placeholderTextColor="rgba(255,255,255,0.2)" style={s.fieldInput} />

            <TextInput value={receiverPhone} onChangeText={setReceiverPhone}
              placeholder="Receiver's Phone Number *" placeholderTextColor="rgba(255,255,255,0.2)"
              style={s.fieldInput} keyboardType="phone-pad" />

            <TextInput value={receiverAddress} onChangeText={setReceiverAddress}
              placeholder="Street Address *" placeholderTextColor="rgba(255,255,255,0.2)" style={s.fieldInput} />

            <View style={s.rowInputs}>
              <TextInput value={receiverCity} onChangeText={setReceiverCity}
                placeholder="City *" placeholderTextColor="rgba(255,255,255,0.2)"
                style={[s.fieldInput, { flex: 1 }]} />
              <TextInput value={receiverZip} onChangeText={setReceiverZip}
                placeholder="ZIP *" placeholderTextColor="rgba(255,255,255,0.2)"
                style={[s.fieldInput, { flex: 0.7 }]} keyboardType="number-pad" maxLength={10} />
            </View>

            <Text style={s.label}>Country</Text>
            <CountryPickerRN value={receiverCountry} onChange={setReceiverCountry} accent={accentAlt} />

            {/* International auto-detected hint */}
            {isIntl && (
              <FadeInView duration={250} style={s.intlHint}>
                <Ionicons name="globe-outline" size={14} color={PURPLE} />
                <Text style={s.intlHintText}>
                  Detected: <Text style={{ color: PURPLE, fontWeight: "600" }}>International Shipment</Text> — freight mode and customs docs will be requested next.
                </Text>
              </FadeInView>
            )}

            <View style={{ flexDirection: "row", gap: 10, marginTop: 8 }}>
              <BackBtn to={1} />
              <Pressable onPress={() => setStep(3)} disabled={!canStep2}
                style={({ pressed }) => [s.btn, { flex: 1, opacity: pressed || !canStep2 ? 0.5 : 1 }]}>
                <LinearGradient colors={isIntl ? [PURPLE, "#6B21D8"] : [CYAN, PURPLE]}
                  start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }} style={s.btnGradient}>
                  <Text style={s.btnText}>Next →</Text>
                </LinearGradient>
              </Pressable>
            </View>
          </FadeInView>
        )}

        {/* ── STEP 3 — Package (local) ──────────────────────────────────── */}
        {!confirmedAwb && step === 3 && !isIntl && (
          <FadeInView fromY={16} style={s.card}>
            <Text style={s.cardTitle}>Package Details</Text>

            <Text style={s.label}>Weight (kg)</Text>
            <View style={s.inputWrap}>
              <Ionicons name="scale-outline" size={16} color="rgba(255,255,255,0.3)" />
              <TextInput value={weight} onChangeText={setWeight} placeholder="e.g. 1.5"
                placeholderTextColor="rgba(255,255,255,0.2)" style={s.input} keyboardType="decimal-pad" />
            </View>

            <Text style={s.label}>Package Description</Text>
            <View style={s.inputWrap}>
              <Ionicons name="cube-outline" size={16} color="rgba(255,255,255,0.3)" />
              <TextInput value={description} onChangeText={setDescription} placeholder="e.g. Electronics, Clothes"
                placeholderTextColor="rgba(255,255,255,0.2)" style={s.input} />
            </View>

            <View style={s.toggleRow}>
              <View style={{ flex: 1 }}>
                <Text style={s.toggleLabel}>Cash on Delivery (COD)</Text>
                <Text style={s.toggleSub}>Recipient pays on delivery</Text>
              </View>
              <Switch value={isCOD} onValueChange={setIsCOD}
                trackColor={{ false: BORDER, true: AMBER + "60" }} thumbColor={isCOD ? AMBER : "rgba(255,255,255,0.3)"} />
            </View>
            {isCOD && (
              <View style={[s.inputWrap, { marginTop: 0 }]}>
                <Text style={[s.input, { color: AMBER, flexGrow: 0 }]}>₱</Text>
                <TextInput value={codAmount} onChangeText={setCodAmount} placeholder="0.00"
                  placeholderTextColor="rgba(255,171,0,0.3)" style={[s.input, { color: AMBER }]} keyboardType="decimal-pad" />
              </View>
            )}

            <View style={s.toggleRow}>
              <View style={{ flex: 1 }}>
                <Text style={s.toggleLabel}>Fragile Item</Text>
                <Text style={s.toggleSub}>Handle with care (+₱30)</Text>
              </View>
              <Switch value={isFragile} onValueChange={setIsFragile}
                trackColor={{ false: BORDER, true: CYAN + "60" }} thumbColor={isFragile ? CYAN : "rgba(255,255,255,0.3)"} />
            </View>

            <View style={{ flexDirection: "row", gap: 10 }}>
              <BackBtn to={2} />
              <Pressable onPress={() => setStep(reviewStep)} disabled={!canStep3}
                style={({ pressed }) => [s.btn, { flex: 1, opacity: pressed || !canStep3 ? 0.5 : 1 }]}>
                <LinearGradient colors={[CYAN, PURPLE]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }} style={s.btnGradient}>
                  <Text style={s.btnText}>Review →</Text>
                </LinearGradient>
              </Pressable>
            </View>
          </FadeInView>
        )}

        {/* ── STEP 3 — Package (international) ─────────────────────────── */}
        {!confirmedAwb && step === 3 && isIntl && (
          <FadeInView fromY={16} style={s.card}>
            <Text style={s.cardTitle}>Box Details</Text>

            <Text style={s.label}>Box Dimensions (cm)</Text>
            <View style={{ flexDirection: "row", gap: 8 }}>
              {([
                { val: boxLength, set: setBoxLength, ph: "Length" },
                { val: boxWidth,  set: setBoxWidth,  ph: "Width"  },
                { val: boxHeight, set: setBoxHeight,  ph: "Height" },
              ] as const).map(({ val, set, ph }) => (
                <View key={ph} style={[s.inputWrap, { flex: 1 }]}>
                  <TextInput value={val} onChangeText={set} placeholder={ph}
                    placeholderTextColor="rgba(255,255,255,0.2)"
                    style={[s.input, { textAlign: "center" }]} keyboardType="decimal-pad" />
                </View>
              ))}
            </View>

            <Text style={s.label}>Actual Weight (kg)</Text>
            <View style={s.inputWrap}>
              <Ionicons name="scale-outline" size={16} color="rgba(255,255,255,0.3)" />
              <TextInput value={weight} onChangeText={setWeight} placeholder="e.g. 20.5"
                placeholderTextColor="rgba(255,255,255,0.2)" style={s.input} keyboardType="decimal-pad" />
            </View>

            <Text style={s.label}>Contents Description</Text>
            <View style={s.inputWrap}>
              <Ionicons name="cube-outline" size={16} color="rgba(255,255,255,0.3)" />
              <TextInput value={contents} onChangeText={setContents}
                placeholder="e.g. Clothes, canned goods, gadgets"
                placeholderTextColor="rgba(255,255,255,0.2)" style={s.input} multiline />
            </View>

            <Text style={s.label}>Declared Value (PHP) — for Customs</Text>
            <View style={s.inputWrap}>
              <Text style={{ fontSize: 14, color: AMBER, fontFamily: "JetBrainsMono-Regular" }}>₱</Text>
              <TextInput value={declaredValue} onChangeText={setDeclaredValue} placeholder="e.g. 15000"
                placeholderTextColor="rgba(255,171,0,0.3)" style={[s.input, { color: AMBER }]} keyboardType="decimal-pad" />
            </View>
            <Text style={s.fieldNote}>Must reflect actual contents value for customs clearance.</Text>

            <Text style={s.label}>Freight Mode</Text>
            <View style={{ flexDirection: "row", gap: 10 }}>
              {([
                { val: "sea" as FreightMode, icon: "boat-outline",     label: "Sea Freight", sub: SEA_DAYS, color: CYAN,  note: "Most economical" },
                { val: "air" as FreightMode, icon: "airplane-outline",  label: "Air Freight", sub: AIR_DAYS, color: AMBER, note: "+₱800 premium" },
              ] as const).map(opt => (
                <Pressable key={opt.val} onPress={() => setFreightMode(opt.val)}
                  style={[s.freightOption, freightMode === opt.val && { borderColor: opt.color + "60", backgroundColor: opt.color + "0F" }]}>
                  <Ionicons name={opt.icon as never} size={20} color={freightMode === opt.val ? opt.color : "rgba(255,255,255,0.35)"} />
                  <Text style={[s.freightLabel, { color: freightMode === opt.val ? opt.color : "rgba(255,255,255,0.7)" }]}>{opt.label}</Text>
                  <Text style={s.freightSub}>{opt.sub}</Text>
                  <Text style={[s.freightNote, { color: freightMode === opt.val ? opt.color + "AA" : "rgba(255,255,255,0.25)" }]}>{opt.note}</Text>
                </Pressable>
              ))}
            </View>

            <View style={{ flexDirection: "row", gap: 10 }}>
              <BackBtn to={2} />
              <Pressable onPress={() => setStep(passportStep)} disabled={!canStep3}
                style={({ pressed }) => [s.btn, { flex: 1, opacity: pressed || !canStep3 ? 0.5 : 1 }]}>
                <LinearGradient colors={[PURPLE, "#6B21D8"]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }} style={s.btnGradient}>
                  <Text style={s.btnText}>Next →</Text>
                </LinearGradient>
              </Pressable>
            </View>
          </FadeInView>
        )}

        {/* ── STEP 4 — Passport (international only) ───────────────────── */}
        {!confirmedAwb && step === passportStep && isIntl && (
          <FadeInView fromY={16} style={s.card}>
            <Text style={s.cardTitle}>Receiver's Passport</Text>

            <View style={s.passportInfoBox}>
              <Ionicons name="information-circle-outline" size={15} color={PURPLE} />
              <Text style={s.passportInfoText}>
                Required for customs clearance in {receiverCountryInfo?.label ?? receiverCountry}. The receiver's passport bio-data page must be valid and clearly photographed.
              </Text>
            </View>

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

            {passportUri ? (
              <FadeInView duration={200}>
                <Image source={{ uri: passportUri }} style={s.passportPreview} resizeMode="cover" />
                <View style={s.passportPreviewRow}>
                  <Ionicons name="checkmark-circle" size={16} color={GREEN} />
                  <Text style={s.passportPreviewText}>Passport uploaded</Text>
                  <Pressable onPress={() => setPassportUri(null)}>
                    <Text style={s.retakeText}>Retake</Text>
                  </Pressable>
                </View>
              </FadeInView>
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
              <BackBtn to={3} />
              <Pressable onPress={() => setStep(reviewStep)} disabled={!canStep4}
                style={({ pressed }) => [s.btn, { flex: 1, opacity: pressed || !canStep4 ? 0.5 : 1 }]}>
                <LinearGradient colors={[PURPLE, "#6B21D8"]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }} style={s.btnGradient}>
                  <Text style={s.btnText}>Review →</Text>
                </LinearGradient>
              </Pressable>
            </View>
          </FadeInView>
        )}

        {/* ── STEP 4 (local) / STEP 5 (intl) — Review & Confirm ────────── */}
        {!confirmedAwb && step === reviewStep && (
          <FadeInView fromY={16} style={s.card}>
            <Text style={s.cardTitle}>Review Booking</Text>

            <View style={[s.serviceTypeBadge, isIntl ? s.badgeIntl : s.badgeLocal]}>
              <Ionicons name={isIntl ? "globe-outline" : "home-outline"} size={13} color={isIntl ? PURPLE : GREEN} />
              <Text style={[s.serviceTypeText, { color: isIntl ? PURPLE : GREEN }]}>
                {isIntl
                  ? `International · ${senderCountryInfo?.flag ?? ""} ${senderCountryInfo?.label ?? senderCountry} → ${receiverCountryInfo?.flag ?? ""} ${receiverCountryInfo?.label ?? receiverCountry}${isIntl ? ` · ${freightMode === "sea" ? "Sea" : "Air"} Freight` : ""}`
                  : "Local Delivery"}
              </Text>
            </View>

            {/* Sender block */}
            <View style={s.reviewBlock}>
              <Text style={[s.reviewBlockTitle, { color: CYAN }]}>Sender</Text>
              <Text style={s.reviewBlockName}>{senderName}</Text>
              <Text style={s.reviewBlockAddr}>{senderPhone}</Text>
              <Text style={s.reviewBlockAddr}>{senderAddress}</Text>
              <Text style={s.reviewBlockAddr}>{senderCity}, {senderZip} · {senderCountryInfo?.flag} {senderCountryInfo?.label}</Text>
            </View>

            {/* Receiver block */}
            <View style={[s.reviewBlock, { borderColor: accentAlt + "30" }]}>
              <Text style={[s.reviewBlockTitle, { color: accentAlt }]}>Receiver</Text>
              <Text style={s.reviewBlockName}>{receiverName}</Text>
              <Text style={s.reviewBlockAddr}>{receiverPhone}</Text>
              <Text style={s.reviewBlockAddr}>{receiverAddress}</Text>
              <Text style={s.reviewBlockAddr}>{receiverCity}, {receiverZip} · {receiverCountryInfo?.flag} {receiverCountryInfo?.label}</Text>
            </View>

            {/* Package rows */}
            {!isIntl && ([
              { label: "Weight",      value: `${weight} kg` },
              { label: "Description", value: description || "—" },
              { label: "COD",         value: isCOD ? `₱${codAmount || "0"}` : "No" },
              { label: "Fragile",     value: isFragile ? "Yes (+₱30)" : "No" },
            ]).map(r => (
              <View key={r.label} style={s.reviewRow}>
                <Text style={s.reviewLabel}>{r.label}</Text>
                <Text style={s.reviewValue}>{r.value}</Text>
              </View>
            ))}

            {isIntl && ([
              { label: "Dimensions",     value: `${boxLength} × ${boxWidth} × ${boxHeight} cm` },
              { label: "Weight",         value: `${weight} kg` },
              { label: "Contents",       value: contents || "—" },
              { label: "Declared Value", value: `₱${declaredValue}` },
              { label: "Freight Mode",   value: freightMode === "sea" ? `Sea Freight (${SEA_DAYS})` : `Air Freight (${AIR_DAYS})` },
              { label: "Receiver ID",    value: passportUri ? "✓ Passport uploaded" : "—" },
            ]).map(r => (
              <View key={r.label} style={s.reviewRow}>
                <Text style={s.reviewLabel}>{r.label}</Text>
                <Text style={s.reviewValue}>{r.value}</Text>
              </View>
            ))}

            {/* Tier discount */}
            {getTier(loyaltyPoints).discount > 0 && (
              <View style={s.reviewRow}>
                <Text style={s.reviewLabel}>{getTier(loyaltyPoints).label} Discount</Text>
                <Text style={[s.reviewValue, { color: GREEN }]}>
                  −{getTier(loyaltyPoints).discount}% (−₱{(calcBaseTotal() - applyTierDiscount(calcBaseTotal(), loyaltyPoints)).toFixed(2)})
                </Text>
              </View>
            )}

            {/* Loyalty redemption */}
            {loyaltyPoints >= REDEMPTION_MIN && (
              <View style={s.loyaltyRedeemRow}>
                <View style={{ flex: 1 }}>
                  <Text style={s.loyaltyRedeemTitle}>Use Loyalty Points</Text>
                  <Text style={s.loyaltyRedeemSub}>{loyaltyPoints} pts available · worth {ptsToPhp(loyaltyPoints)}</Text>
                </View>
                <Switch value={redeemPoints} onValueChange={v => {
                  setRedeemPoints(v);
                  if (v) setRedeemAmount(Math.floor(maxRedemptionValue(loyaltyPoints, calcBaseTotal()) * 10) / 10);
                  else setRedeemAmount(0);
                }} thumbColor={redeemPoints ? GREEN : "rgba(255,255,255,0.3)"}
                  trackColor={{ false: BORDER, true: "rgba(0,255,136,0.3)" }} />
              </View>
            )}

            {redeemPoints && redeemAmount > 0 && (
              <View style={s.reviewRow}>
                <Text style={s.reviewLabel}>Points Redeemed</Text>
                <Text style={[s.reviewValue, { color: GREEN }]}>−₱{redeemAmount.toFixed(2)} ({ptsForDiscount(redeemAmount)} pts)</Text>
              </View>
            )}

            <View style={[s.reviewRow, s.totalRow]}>
              <Text style={s.totalLabel}>Estimated Total</Text>
              <Text style={[s.totalValue, { color: isIntl ? PURPLE : GREEN }]}>₱{calcTotal().toFixed(2)}</Text>
            </View>

            <View style={s.earnPtsRow}>
              <Ionicons name="star-outline" size={13} color={accentAlt} />
              <Text style={[s.earnPtsText, { color: accentAlt + "99" }]}>
                You'll earn +{calcPointsToEarn()} loyalty points for this booking
              </Text>
            </View>

            {isIntl && (
              <View style={s.transitNote}>
                <Ionicons name="time-outline" size={13} color="rgba(168,85,247,0.6)" />
                <Text style={s.transitNoteText}>
                  Estimated transit: {freightMode === "sea" ? SEA_DAYS : AIR_DAYS}. Customs clearance may add 3–7 days.
                </Text>
              </View>
            )}

            <View style={{ flexDirection: "row", gap: 10, marginTop: 4 }}>
              <BackBtn to={isIntl ? passportStep : 3} />
              <Pressable onPress={handleBook} disabled={isLoading}
                style={({ pressed }) => [s.btn, { flex: 1, opacity: pressed || isLoading ? 0.5 : 1 }]}>
                <LinearGradient colors={isIntl ? [PURPLE, "#6B21D8"] : [GREEN, CYAN]}
                  start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }} style={s.btnGradient}>
                  <Text style={s.btnText}>{isLoading ? "Booking..." : isIntl ? "Book Balikbayan Box" : "Confirm Booking"}</Text>
                </LinearGradient>
              </Pressable>
            </View>
          </FadeInView>
        )}

        {/* ── Booking Confirmed ─────────────────────────────────────────── */}
        {confirmedAwb && (
          <FadeInView fromY={16} style={s.successCard}>
            <LinearGradient
              colors={isIntl ? ["rgba(168,85,247,0.15)", "rgba(168,85,247,0.04)"] : ["rgba(0,255,136,0.15)", "rgba(0,255,136,0.04)"]}
              style={s.successGradient}
            >
              <View style={[s.successIcon, { backgroundColor: accentAlt + "25" }]}>
                <Ionicons name="checkmark-circle" size={44} color={accentAlt} />
              </View>
              <Text style={s.successTitle}>Booking Confirmed!</Text>
              <Text style={s.successSub}>
                {isIntl ? "Your Balikbayan Box has been registered." : "Your shipment has been booked."}
              </Text>

              <View style={s.awbBox}>
                <Text style={s.awbBoxLabel}>Tracking Number</Text>
                <Text style={[s.awbBoxValue, { color: isIntl ? PURPLE : CYAN }]}>{confirmedAwb}</Text>
              </View>

              <AwbQRCode awb={confirmedAwb} size={180} accent={isIntl ? PURPLE : CYAN} />

              <View style={s.successRows}>
                {[
                  { icon: "time-outline",     label: "Status",        value: "Confirmed" },
                  { icon: "star-outline",     label: "Points Earned", value: `+${calcPointsToEarn()} pts` },
                  { icon: "navigate-outline", label: "From",          value: `${senderName} · ${senderCity}` },
                  { icon: "call-outline",     label: "Sender Phone",  value: senderPhone },
                  { icon: "location-outline", label: "To",            value: `${receiverName} · ${receiverCity} (${receiverCountryInfo?.flag} ${receiverCountryInfo?.label ?? receiverCountry})` },
                  { icon: "call-outline",     label: "Receiver Phone",value: receiverPhone },
                ].map(r => (
                  <View key={r.label} style={s.successRow}>
                    <Ionicons name={r.icon as any} size={13} color="rgba(255,255,255,0.3)" />
                    <Text style={s.successRowLabel}>{r.label}</Text>
                    <Text style={[s.successRowValue, r.label === "Points Earned" && { color: accentAlt }]}>{r.value}</Text>
                  </View>
                ))}
              </View>

              {/* Track Pickup — primary CTA after booking */}
              <Pressable
                onPress={() => navigation.navigate("Collection", { awb: confirmedAwb, type: isIntl ? "international" : "local" })}
                style={[s.btn, { marginTop: 4 }]}
              >
                <LinearGradient
                  colors={isIntl ? [PURPLE, "#6B21D8"] : [GREEN, CYAN]}
                  start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }}
                  style={s.btnGradient}
                >
                  <Ionicons name="locate-outline" size={16} color="#050810" />
                  <Text style={s.btnText}>Track Pickup</Text>
                </LinearGradient>
              </Pressable>

              <Pressable onPress={handleBookAnother} style={[s.btn, s.btnSecondary]}>
                <Text style={s.btnSecondaryText}>Book Another Shipment</Text>
              </Pressable>
            </LinearGradient>
          </FadeInView>
        )}

        <Toast message={toastMessage} type={toastType} visible={toastVisible} onHide={() => setToastVisible(false)} />
      </ScrollView>
    </KeyboardAvoidingView>
  );
}

const s = StyleSheet.create({
  hero:              { paddingHorizontal: 20, paddingTop: 52, paddingBottom: 12 },
  heroTitle:         { fontSize: 26, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  heroSub:           { fontSize: 13, color: "rgba(255,255,255,0.4)", marginTop: 4 },

  typeBadgeWrap:     { paddingHorizontal: 16, marginBottom: 10 },
  typeBadge:         { flexDirection: "row", alignItems: "center", gap: 6, alignSelf: "flex-start", paddingHorizontal: 12, paddingVertical: 6, borderRadius: 20, borderWidth: 1 },
  typeBadgeLocal:    { backgroundColor: "rgba(0,255,136,0.08)", borderColor: "rgba(0,255,136,0.25)" },
  typeBadgeIntl:     { backgroundColor: "rgba(168,85,247,0.08)", borderColor: "rgba(168,85,247,0.25)" },
  typeBadgeText:     { fontSize: 12, fontWeight: "600" },

  stepRow:           { flexDirection: "row", gap: 6, paddingHorizontal: 20, marginBottom: 16 },
  stepDot:           { flex: 1, height: 3, borderRadius: 2 },

  card:              { marginHorizontal: 16, marginBottom: 16, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 16, padding: 20, gap: 10 },

  sectionHeader:     { flexDirection: "row", alignItems: "center", gap: 8, marginBottom: 4 },
  sectionHeading:    { fontSize: 13, fontWeight: "700", letterSpacing: 0.5, textTransform: "uppercase" },
  sectionDivider:    { height: 1, backgroundColor: BORDER, marginVertical: 4 },

  gpsBtn:            { flexDirection: "row", alignItems: "center", gap: 8, backgroundColor: "rgba(0,229,255,0.06)", borderWidth: 1, borderColor: "rgba(0,229,255,0.2)", borderRadius: 10, paddingHorizontal: 14, paddingVertical: 10 },
  gpsBtnActive:      { backgroundColor: "rgba(0,255,136,0.06)", borderColor: "rgba(0,255,136,0.25)" },
  gpsBtnText:        { flex: 1, fontSize: 12, color: CYAN, fontFamily: "JetBrainsMono-Regular" },

  fieldInput:        { backgroundColor: "rgba(255,255,255,0.04)", borderWidth: 1, borderColor: BORDER, borderRadius: 10, paddingHorizontal: 14, paddingVertical: 11, color: "#FFF", fontSize: 14 },
  rowInputs:         { flexDirection: "row", gap: 10 },

  label:             { fontSize: 11, fontWeight: "600", color: "rgba(255,255,255,0.4)", textTransform: "uppercase", letterSpacing: 0.8, marginTop: 4 },

  inputWrap:         { flexDirection: "row", alignItems: "center", gap: 10, backgroundColor: "rgba(255,255,255,0.04)", borderWidth: 1, borderColor: BORDER, borderRadius: 10, paddingHorizontal: 12, paddingVertical: 10, marginTop: 4 },
  input:             { flex: 1, color: "#FFF", fontSize: 14 },

  countryPickerDrop: { backgroundColor: "#0D1117", borderWidth: 1, borderColor: BORDER, borderRadius: 12, marginTop: 4, overflow: "hidden" },
  countrySearchRow:  { flexDirection: "row", alignItems: "center", gap: 8, paddingHorizontal: 12, paddingVertical: 10, borderBottomWidth: 1, borderBottomColor: BORDER },
  countrySearchInput:{ flex: 1, color: "#FFF", fontSize: 13 },
  countryGroupLabel: { fontSize: 10, color: "rgba(255,255,255,0.25)", fontWeight: "700", textTransform: "uppercase", letterSpacing: 1, paddingHorizontal: 14, paddingVertical: 6 },
  countryOption:     { flexDirection: "row", alignItems: "center", gap: 10, paddingHorizontal: 14, paddingVertical: 10 },
  countryOptionText: { flex: 1, fontSize: 13, color: "rgba(255,255,255,0.7)" },
  countryCode:       { fontSize: 11, color: "rgba(255,255,255,0.25)", fontFamily: "JetBrainsMono-Regular" },

  intlHint:          { flexDirection: "row", alignItems: "flex-start", gap: 8, backgroundColor: "rgba(168,85,247,0.06)", borderWidth: 1, borderColor: "rgba(168,85,247,0.2)", borderRadius: 10, padding: 12 },
  intlHintText:      { flex: 1, fontSize: 12, color: "rgba(255,255,255,0.5)", lineHeight: 18 },

  cardTitle:         { fontSize: 16, fontWeight: "700", color: "#FFF", marginBottom: 4 },
  fieldNote:         { fontSize: 11, color: "rgba(255,255,255,0.3)", lineHeight: 16, marginTop: -4 },

  toggleRow:         { flexDirection: "row", alignItems: "center", backgroundColor: "rgba(255,255,255,0.02)", borderWidth: 1, borderColor: BORDER, borderRadius: 10, padding: 12, marginTop: 4 },
  toggleLabel:       { fontSize: 13, fontWeight: "600", color: "#FFF" },
  toggleSub:         { fontSize: 11, color: "rgba(255,255,255,0.35)", marginTop: 2 },

  freightOption:     { flex: 1, alignItems: "center", gap: 4, backgroundColor: "rgba(255,255,255,0.02)", borderWidth: 1, borderColor: BORDER, borderRadius: 12, padding: 14 },
  freightLabel:      { fontSize: 13, fontWeight: "600" },
  freightSub:        { fontSize: 11, color: "rgba(255,255,255,0.35)" },
  freightNote:       { fontSize: 10, marginTop: 2 },

  passportInfoBox:   { flexDirection: "row", alignItems: "flex-start", gap: 10, backgroundColor: "rgba(168,85,247,0.06)", borderWidth: 1, borderColor: "rgba(168,85,247,0.2)", borderRadius: 10, padding: 12 },
  passportInfoText:  { flex: 1, fontSize: 12, color: "rgba(255,255,255,0.55)", lineHeight: 18 },
  reqBox:            { backgroundColor: "rgba(255,255,255,0.02)", borderWidth: 1, borderColor: BORDER, borderRadius: 10, padding: 12, gap: 8 },
  reqRow:            { flexDirection: "row", alignItems: "flex-start", gap: 8 },
  reqText:           { flex: 1, fontSize: 12, color: "rgba(255,255,255,0.5)", lineHeight: 18 },
  passportPreview:   { width: "100%", height: 180, borderRadius: 12, backgroundColor: "rgba(168,85,247,0.1)" },
  passportPreviewRow:{ flexDirection: "row", alignItems: "center", gap: 8, paddingTop: 10 },
  passportPreviewText:{ flex: 1, fontSize: 13, color: GREEN },
  retakeText:        { fontSize: 12, color: PURPLE, textDecorationLine: "underline" },
  uploadZone:        { alignItems: "center", gap: 8, backgroundColor: "rgba(168,85,247,0.04)", borderWidth: 1, borderColor: "rgba(168,85,247,0.15)", borderRadius: 14, borderStyle: "dashed", paddingVertical: 28 },
  uploadTitle:       { fontSize: 14, fontWeight: "600", color: "rgba(255,255,255,0.6)" },
  uploadSub:         { fontSize: 11, color: "rgba(255,255,255,0.25)" },
  uploadBtns:        { flexDirection: "row", gap: 12, marginTop: 8 },
  uploadBtn:         { flexDirection: "row", alignItems: "center", gap: 6, paddingHorizontal: 16, paddingVertical: 8, borderWidth: 1, borderRadius: 8 },
  uploadBtnText:     { fontSize: 12, fontWeight: "600" },

  serviceTypeBadge:  { flexDirection: "row", alignItems: "center", gap: 8, borderRadius: 10, paddingHorizontal: 12, paddingVertical: 8, borderWidth: 1, marginBottom: 4 },
  badgeLocal:        { backgroundColor: "rgba(0,255,136,0.06)", borderColor: "rgba(0,255,136,0.2)" },
  badgeIntl:         { backgroundColor: "rgba(168,85,247,0.06)", borderColor: "rgba(168,85,247,0.2)" },
  serviceTypeText:   { fontSize: 12, fontWeight: "600", flex: 1, flexWrap: "wrap" },

  reviewBlock:       { backgroundColor: "rgba(255,255,255,0.02)", borderWidth: 1, borderColor: "rgba(0,229,255,0.2)", borderRadius: 10, padding: 12, gap: 3 },
  reviewBlockTitle:  { fontSize: 10, fontWeight: "700", textTransform: "uppercase", letterSpacing: 1, marginBottom: 4 },
  reviewBlockName:   { fontSize: 14, fontWeight: "600", color: "#FFF" },
  reviewBlockAddr:   { fontSize: 12, color: "rgba(255,255,255,0.5)" },
  reviewRow:         { flexDirection: "row", justifyContent: "space-between", alignItems: "center", paddingVertical: 6, borderBottomWidth: 1, borderBottomColor: BORDER },
  reviewLabel:       { fontSize: 12, color: "rgba(255,255,255,0.4)" },
  reviewValue:       { fontSize: 12, color: "#FFF", fontWeight: "600", textAlign: "right", flex: 1, marginLeft: 12 },
  totalRow:          { borderBottomWidth: 0, paddingVertical: 10, marginTop: 4 },
  totalLabel:        { fontSize: 15, fontWeight: "700", color: "#FFF" },
  totalValue:        { fontSize: 20, fontWeight: "700" },

  loyaltyRedeemRow:  { flexDirection: "row", alignItems: "center", backgroundColor: "rgba(0,255,136,0.04)", borderWidth: 1, borderColor: "rgba(0,255,136,0.15)", borderRadius: 10, padding: 12, gap: 12 },
  loyaltyRedeemTitle:{ fontSize: 13, fontWeight: "600", color: "#FFF" },
  loyaltyRedeemSub:  { fontSize: 11, color: "rgba(255,255,255,0.35)", marginTop: 2 },

  earnPtsRow:        { flexDirection: "row", alignItems: "center", gap: 8, paddingVertical: 8 },
  earnPtsText:       { fontSize: 12 },
  transitNote:       { flexDirection: "row", alignItems: "flex-start", gap: 8, backgroundColor: "rgba(168,85,247,0.04)", borderRadius: 8, padding: 10 },
  transitNoteText:   { flex: 1, fontSize: 11, color: "rgba(168,85,247,0.7)", lineHeight: 16 },

  btn:               { borderRadius: 12, overflow: "hidden" },
  btnGradient:       { flexDirection: "row", gap: 8, paddingVertical: 14, alignItems: "center", justifyContent: "center", borderRadius: 12 },
  btnText:           { fontSize: 14, fontWeight: "700", color: "#050810" },
  btnSecondary:      { paddingVertical: 13, alignItems: "center", justifyContent: "center", borderRadius: 12, borderWidth: 1, borderColor: "rgba(255,255,255,0.12)", backgroundColor: "rgba(255,255,255,0.04)", marginTop: 8 },
  btnSecondaryText:  { fontSize: 14, fontWeight: "600", color: "rgba(255,255,255,0.55)" },

  successCard:       { margin: 16 },
  successGradient:   { borderRadius: 20, padding: 24, alignItems: "center", gap: 12, borderWidth: 1, borderColor: BORDER },
  successIcon:       { width: 80, height: 80, borderRadius: 40, alignItems: "center", justifyContent: "center" },
  successTitle:      { fontSize: 22, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  successSub:        { fontSize: 13, color: "rgba(255,255,255,0.4)", textAlign: "center" },
  awbBox:            { backgroundColor: "rgba(255,255,255,0.04)", borderWidth: 1, borderColor: BORDER, borderRadius: 12, paddingHorizontal: 20, paddingVertical: 12, alignItems: "center", width: "100%" },
  awbBoxLabel:       { fontSize: 10, color: "rgba(255,255,255,0.35)", textTransform: "uppercase", letterSpacing: 1 },
  awbBoxValue:       { fontSize: 20, fontWeight: "700", fontFamily: "JetBrainsMono-Regular", marginTop: 4 },
  successRows:       { width: "100%", gap: 2 },
  successRow:        { flexDirection: "row", alignItems: "center", gap: 10, paddingVertical: 6, borderBottomWidth: 1, borderBottomColor: BORDER },
  successRowLabel:   { fontSize: 12, color: "rgba(255,255,255,0.35)", flex: 1 },
  successRowValue:   { fontSize: 12, color: "#FFF", fontWeight: "600" },
});

/**
 * What an order was made of.
 *
 * The grand total alone told a customer what they owed and never what for —
 * which reads as a mistake the moment a modifier has folded a large-size delta
 * into a line price and the number no longer matches the menu.
 *
 * The rail is named out loud on the last line. A money panel that does not say
 * how it is paid is what invites the assumption that a balance sits behind it.
 */
import { Text, View } from "react-native";

import type { PaymentStatus } from "@/api/orders";
import { peso } from "@/money";
import { theme } from "@/theme";

export interface ReceiptProps {
  goods_total_cents: number;
  delivery_fee_cents: number;
  tip_cents: number;
  grand_total_cents: number;
  /**
   * What the courier still collects at the door — server-computed, and equal to
   * the grand total for a COD order. This, not the grand total, is the number
   * to put in front of someone about to find cash: telling a customer who has
   * already paid by card to have the full amount ready is the one thing this
   * line must never do.
   */
  cod_amount_cents: number;
  payment_status: PaymentStatus;
  /** Cash is only still owed while the order is in flight. */
  settled: boolean;
}

/**
 * The one sentence that says who is owed what. Split out so the rule is
 * testable without rendering: getting it wrong sends a customer to the door
 * with cash they do not need, or without cash they do.
 */
export function settlementLine(p: {
  cod_amount_cents: number;
  payment_status: PaymentStatus;
  settled: boolean;
}): { text: string; owed: boolean } {
  // A deployed service that predates `cod_amount_cents` omits the field rather
  // than sending null, and the non-optional type on TrackResponse means nothing
  // rejects that at the boundary. Reading it as a number anyway is what put
  // "Please have ₱NaN in cash ready" in front of a customer who had paid by
  // card. Absent is a third state here, distinct from 0 and from a balance, and
  // it has to be answered without inventing the number the server never sent.
  const cod = Number.isFinite(p.cod_amount_cents) ? p.cod_amount_cents : null;

  if (cod === 0) {
    // Fully prepaid. `captured` is money actually taken; `authorized` is a hold
    // that becomes a charge when a courier accepts. Neither leaves anything to
    // find at the door, and saying so is the point.
    if (p.payment_status === "captured") return { text: "Paid by card", owed: false };
    if (p.payment_status === "authorized")
      return { text: "Held on your card — charged when a courier accepts", owed: false };
    return { text: "Nothing to pay at the door", owed: false };
  }

  if (cod === null) {
    // The rail still answers the only question that matters at the door. A card
    // order carries a hold or a charge, and neither leaves cash to find — what
    // is unknown is the amount, not who has already paid.
    if (p.payment_status === "captured") return { text: "Paid by card", owed: false };
    if (p.payment_status === "authorized")
      return { text: "Held on your card — charged when a courier accepts", owed: false };
    if (p.settled) return { text: "Paid in cash on delivery", owed: false };
    // Still warn, but name no figure. An amount this screen worked out itself
    // is exactly the arithmetic `cod_amount_cents` exists to keep it out of.
    return { text: "Please have the order total in cash ready.", owed: true };
  }

  if (p.settled) return { text: "Paid in cash on delivery", owed: false };
  return {
    text: `Please have ${peso(cod)} in cash ready.`,
    owed: true,
  };
}

export function Receipt(p: ReceiptProps) {
  const line = settlementLine(p);
  // The same absent field, subtracted, rendered this row as "Paid online
  // ₱NaN". With nothing prepaid that can be proven, the row is not shown at
  // all: a money line the screen cannot substantiate is worse than its absence.
  const prepaid_cents =
    Number.isFinite(p.grand_total_cents) && Number.isFinite(p.cod_amount_cents)
      ? p.grand_total_cents - p.cod_amount_cents
      : 0;
  return (
    <View
      style={{
        backgroundColor: theme.surface,
        borderColor: theme.border,
        borderWidth: 1,
        borderRadius: theme.radius.md,
        padding: 14,
        gap: 8,
      }}
    >
      <Line label="Goods" value={peso(p.goods_total_cents)} />
      <Line label="Delivery fee" value={peso(p.delivery_fee_cents)} />
      {/* Zero tip is shown rather than hidden: a missing line reads as a
          number that was rolled into something else. */}
      <Line label="Tip" value={peso(p.tip_cents)} />

      <View style={{ height: 1, backgroundColor: theme.border, marginVertical: 2 }} />

      <View style={{ flexDirection: "row", justifyContent: "space-between" }}>
        <Text style={{ color: theme.text, fontSize: 14, fontWeight: "800" }}>Total</Text>
        <Text style={{ color: theme.text, fontSize: 14, fontWeight: "800" }}>
          {peso(p.grand_total_cents)}
        </Text>
      </View>

      {prepaid_cents > 0 && <Line label="Paid online" value={peso(prepaid_cents)} />}

      <Text style={{ color: line.owed ? theme.amber : theme.faint, fontSize: 12 }}>
        {line.text}
      </Text>
    </View>
  );
}

function Line({ label, value }: { label: string; value: string }) {
  return (
    <View style={{ flexDirection: "row", justifyContent: "space-between" }}>
      <Text style={{ color: theme.muted, fontSize: 13 }}>{label}</Text>
      <Text style={{ color: theme.text, fontSize: 13, fontWeight: "700" }}>{value}</Text>
    </View>
  );
}

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

import { peso } from "@/money";
import { theme } from "@/theme";

export interface ReceiptProps {
  goods_total_cents: number;
  delivery_fee_cents: number;
  tip_cents: number;
  grand_total_cents: number;
  /** Cash is only still owed while the order is in flight. */
  settled: boolean;
}

export function Receipt(p: ReceiptProps) {
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

      <Text style={{ color: p.settled ? theme.faint : theme.amber, fontSize: 12 }}>
        {p.settled
          ? "Paid in cash on delivery"
          : `Please have ${peso(p.grand_total_cents)} in cash ready.`}
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

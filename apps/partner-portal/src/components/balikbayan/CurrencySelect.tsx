"use client";
import { SUPPORTED_CURRENCIES } from "@/lib/data/currencies";

interface Props {
  value: string;
  onChange: (code: string) => void;
  className?: string;
}

export function CurrencySelect({ value, onChange, className }: Props) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className={className}
    >
      {SUPPORTED_CURRENCIES.map((c) => (
        <option key={c.code} value={c.code} style={{ background: "#0d1422" }}>
          {c.code}
        </option>
      ))}
    </select>
  );
}

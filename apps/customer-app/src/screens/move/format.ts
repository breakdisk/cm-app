/** Display formatting for the Move app. Amounts arrive in minor units from the
 *  server and are only ever formatted here — never added up. */

const SYMBOL: Record<string, string> = {
  USD: '$', PHP: '₱', GBP: '£', EUR: '€', AUD: 'A$', CAD: 'C$', SGD: 'S$',
  // No reliable glyph in the system fonts: the riyal sign (U+20C0) renders as a
  // missing-glyph box, per the handoff. Codes read cleanly instead.
  AED: 'AED ', SAR: 'SR ',
};

export function formatMoney(
  cents: number,
  currency: string | null | undefined,
  { whole = false }: { whole?: boolean } = {},
): string {
  const code = (currency ?? '').toUpperCase();
  const abs = Math.abs(Math.round(cents));
  const units = Math.floor(abs / 100);
  const frac = abs % 100;
  const grouped = String(units).replace(/\B(?=(\d{3})+(?!\d))/g, ',');
  const body = whole && frac === 0 ? grouped : `${grouped}.${String(frac).padStart(2, '0')}`;
  const symbol = SYMBOL[code] ?? (code ? `${code} ` : '');
  return `${cents < 0 ? '-' : ''}${symbol}${body}`;
}

export function formatKg(grams: number): string {
  const kg = grams / 1000;
  return `${kg >= 100 ? Math.round(kg).toLocaleString('en-US') : +kg.toFixed(1)} kg`;
}

export function initials(name: string | null | undefined): string {
  const parts = (name ?? '').trim().split(/\s+/).filter(Boolean);
  return parts.slice(0, 2).map((p) => p[0].toUpperCase()).join('') || '·';
}

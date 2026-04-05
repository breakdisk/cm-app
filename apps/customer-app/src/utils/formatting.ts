import { format, formatDistance } from 'date-fns';

export interface FormatDateOptions {
  time?: boolean;
  relative?: boolean;
}

export function formatDate(date: Date, opts: FormatDateOptions = {}): string {
  if (opts.relative) {
    return formatDistance(new Date(date), new Date(), { addSuffix: true });
  }
  return format(new Date(date), opts.time ? 'MMM d, yyyy HH:mm' : 'MMM d, yyyy');
}

export function formatCurrency(amount: number, currency: 'PHP' | 'USD' = 'PHP'): string {
  const formatter = new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency,
  });
  return formatter.format(amount);
}

export function formatPhone(phone: string): string {
  const cleaned = phone.replace(/\D/g, '');
  if (cleaned.startsWith('0')) {
    return '+63' + cleaned.slice(1);
  }
  if (cleaned.startsWith('63')) {
    return '+' + cleaned;
  }
  if (!cleaned.startsWith('+')) {
    return '+' + cleaned;
  }
  return '+' + cleaned;
}

export function formatAWB(awb: string): string {
  return awb.toUpperCase().slice(0, 10);
}

export function formatRouteString(origin: string, destination: string): string {
  return `${origin} → ${destination}`;
}

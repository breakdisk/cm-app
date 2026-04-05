export function validatePhone(phone: string): boolean {
  const cleaned = phone.replace(/\D/g, '');
  return cleaned.length >= 10 && cleaned.length <= 15;
}

export function validateEmail(email: string): boolean {
  const re = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
  return re.test(email);
}

export function validateWeight(weight: number, mode: 'standard' | 'air' | 'sea'): boolean {
  if (weight <= 0) return false;
  const limits = { standard: 50, air: 100, sea: 100 };
  return weight <= limits[mode];
}

export function validateCOD(amount: number): boolean {
  return amount > 0;
}

export function validateAddress(address: string): boolean {
  return address && address.trim().length >= 5;
}

export function validateRecipientName(name: string): boolean {
  return name && name.trim().length >= 2;
}

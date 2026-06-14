import { getOrderClient } from './client';

export interface AddressCode {
  country_code:   string;
  postal_code:    string;
  city:           string;
  state_province: string | null;
  region:         string | null;
  time_zone:      string | null;
}

/** Lookup city/province from a postal code. Returns [] on any error. */
export async function lookupByPostalCode(
  countryCode: string,
  postalCode:  string,
): Promise<AddressCode[]> {
  if (postalCode.length < 3) return [];
  try {
    const res = await getOrderClient().get<{ data: AddressCode[] }>(
      '/v1/address/lookup',
      { params: { country_code: countryCode, postal_code: postalCode, limit: 5 } },
    );
    return res.data.data ?? [];
  } catch {
    return [];
  }
}

/** Type-ahead city prefix search. Returns [] on any error. */
export async function lookupByCity(
  countryCode: string,
  cityPrefix:  string,
  limit = 8,
): Promise<AddressCode[]> {
  if (cityPrefix.length < 2) return [];
  try {
    const res = await getOrderClient().get<{ data: AddressCode[] }>(
      '/v1/address/lookup',
      { params: { country_code: countryCode, city: cityPrefix, limit } },
    );
    return res.data.data ?? [];
  } catch {
    return [];
  }
}

/** Fuzzy search across city + province. Best for province/region type-ahead. */
export async function lookupByQuery(
  countryCode: string,
  query:       string,
  limit = 8,
): Promise<AddressCode[]> {
  if (query.length < 2) return [];
  try {
    const res = await getOrderClient().get<{ data: AddressCode[] }>(
      '/v1/address/lookup',
      { params: { country_code: countryCode, q: query, limit } },
    );
    return res.data.data ?? [];
  } catch {
    return [];
  }
}

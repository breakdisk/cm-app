export interface Currency {
  code: string;
  name: string;
}

export const SUPPORTED_CURRENCIES: Currency[] = [
  { code: "USD", name: "US Dollar"          },
  { code: "AED", name: "UAE Dirham"         },
  { code: "SAR", name: "Saudi Riyal"        },
  { code: "QAR", name: "Qatari Riyal"       },
  { code: "EUR", name: "Euro"               },
  { code: "GBP", name: "British Pound"      },
  { code: "NOK", name: "Norwegian Krone"    },
  { code: "SEK", name: "Swedish Krona"      },
  { code: "CAD", name: "Canadian Dollar"    },
  { code: "AUD", name: "Australian Dollar"  },
  { code: "NZD", name: "New Zealand Dollar" },
  { code: "JPY", name: "Japanese Yen"       },
  { code: "HKD", name: "Hong Kong Dollar"   },
  { code: "SGD", name: "Singapore Dollar"   },
  { code: "KRW", name: "South Korean Won"   },
  { code: "PHP", name: "Philippine Peso"    },
];

export function fmtCurrency(amount: number, currencyCode: string): string {
  try {
    return new Intl.NumberFormat("en-US", {
      style: "currency",
      currency: currencyCode,
      minimumFractionDigits: 0,
      maximumFractionDigits: 2,
    }).format(amount);
  } catch {
    return `${currencyCode} ${amount}`;
  }
}

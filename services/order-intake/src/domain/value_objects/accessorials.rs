//! Accessorial pricing. Server-side and nowhere else, the same rule as
//! `quote_price_cents`: the caller states which accessorials and how many units,
//! never the price.
//!
//! Shape follows `design/Accessorial Config.dc.html` so the consumer app's
//! hardcoded `ACCESSORIALS` object becomes a fetch with no reshaping.

use serde::{Deserialize, Serialize};

use crate::config::{AccessorialBasis, AccessorialsConfig};
use logisticos_types::{Currency, Money};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AccessorialRequest {
    pub code: String,
    #[serde(default)]
    pub units: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AccessorialError {
    #[error("accessorial '{0}' is not offered in this market")]
    NotOffered(String),
    #[error("accessorial '{0}' is priced in a different currency to this tenant")]
    CurrencyMismatch(String),
    #[error("accessorial '{0}' allows at most {1} units")]
    TooManyUnits(String, u16),
}

/// One priced accessorial, for the quote breakdown's `items[]`.
#[derive(Debug, Clone, Serialize)]
pub struct PricedAccessorial {
    pub code: String,
    pub units: u16,
    /// What the customer is charged. Serialised as `amount_cents`, the name the
    /// design's response shape uses.
    #[serde(rename = "amount_cents")]
    pub billed_cents: i64,
    /// What settlement pays. Never serialised: it reveals the take rate, and this
    /// struct is rendered straight into the consumer's quote breakdown.
    #[serde(skip_serializing)]
    pub paid_cents: i64,
}

/// Total for the requested accessorials, or the first offending code.
///
/// Sums `price_accessorials_itemised` rather than repeating the arithmetic — the
/// Review screen's rows and its total cannot drift, the same invariant the
/// carriage fee holds.
pub fn price_accessorials(
    cfg: &AccessorialsConfig,
    currency: Currency,
    requested: &[AccessorialRequest],
) -> Result<Money, AccessorialError> {
    let items = price_accessorials_itemised(cfg, currency, requested)?;
    Ok(Money::new(
        items.iter().map(|p| p.billed_cents).sum(),
        currency,
    ))
}

/// The same calculation, itemised.
pub fn price_accessorials_itemised(
    cfg: &AccessorialsConfig,
    currency: Currency,
    requested: &[AccessorialRequest],
) -> Result<Vec<PricedAccessorial>, AccessorialError> {
    let mut out = Vec::with_capacity(requested.len());

    for req in requested {
        let rate = cfg
            .lookup(&req.code)
            .ok_or_else(|| AccessorialError::NotOffered(req.code.clone()))?;

        if rate.currency != currency.to_string() {
            return Err(AccessorialError::CurrencyMismatch(req.code.clone()));
        }

        let units = match rate.basis {
            AccessorialBasis::Booking => 1,
            // No count stated means no flights stated. Pricing one by default
            // would charge for something the caller never asked for.
            AccessorialBasis::StairFlight => req.units.unwrap_or(0),
        };

        if let Some(max) = rate.max_units {
            if units > max {
                return Err(AccessorialError::TooManyUnits(req.code.clone(), max));
            }
        }

        out.push(PricedAccessorial {
            code: req.code.clone(),
            units,
            billed_cents: rate.billed_cents.saturating_mul(units as i64),
            paid_cents: rate.paid_cents.saturating_mul(units as i64),
        });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AccessorialRate;

    fn php_card() -> AccessorialsConfig {
        AccessorialsConfig {
            // billed = design USD billed x 58; paid = design USD paid x 58.
            helper: Some(AccessorialRate {
                billed_cents: 81_200,
                paid_cents: 63_800,
                currency: "PHP".into(),
                basis: AccessorialBasis::StairFlight,
                max_units: Some(8),
            }),
            assembly: Some(AccessorialRate {
                billed_cents: 150_800,
                paid_cents: 116_000,
                currency: "PHP".into(),
                basis: AccessorialBasis::Booking,
                max_units: None,
            }),
            haulaway: Some(AccessorialRate {
                billed_cents: 226_200,
                paid_cents: 150_800,
                currency: "PHP".into(),
                basis: AccessorialBasis::Booking,
                max_units: None,
            }),
            // Deliberately unset, to exercise the not-offered path.
            carbon_offset: None,
        }
    }

    fn req(code: &str, units: Option<u16>) -> AccessorialRequest {
        AccessorialRequest { code: code.into(), units }
    }

    /// The design's worked example: 2 flights of helper plus haul-away.
    #[test]
    fn it_prices_the_designs_worked_example() {
        let total = price_accessorials(
            &php_card(),
            Currency::PHP,
            &[req("helper", Some(2)), req("haulaway", None)],
        )
        .expect("both are offered");
        assert_eq!(total.amount, 162_400 + 226_200);
    }

    /// An unconfigured accessorial is an error naming the code, never a silent
    /// zero that quietly gives it away.
    #[test]
    fn an_unconfigured_accessorial_is_an_error_not_a_free_one() {
        let err = price_accessorials(&php_card(), Currency::PHP, &[req("carbon_offset", None)])
            .expect_err("not offered in this market");
        assert_eq!(err, AccessorialError::NotOffered("carbon_offset".into()));
    }

    /// Helper caps at 8 flights. Without the cap a caller states 400.
    #[test]
    fn too_many_units_is_an_error_not_a_clamp() {
        let err = price_accessorials(&php_card(), Currency::PHP, &[req("helper", Some(20))])
            .expect_err("over the cap");
        assert_eq!(err, AccessorialError::TooManyUnits("helper".into(), 8));
    }

    /// A card in the wrong currency is refused rather than charged at face
    /// value, so a misconfigured market cannot underbill.
    #[test]
    fn a_currency_mismatch_is_refused() {
        let err = price_accessorials(&php_card(), Currency::AED, &[req("assembly", None)])
            .expect_err("PHP card, AED tenant");
        assert_eq!(err, AccessorialError::CurrencyMismatch("assembly".into()));
    }

    /// A stair-flight accessorial with no unit count prices at zero units, not
    /// at one — the caller said nothing about flights.
    #[test]
    fn a_stair_flight_accessorial_with_no_count_is_zero() {
        let total =
            price_accessorials(&php_card(), Currency::PHP, &[req("helper", None)]).expect("offered");
        assert_eq!(total.amount, 0);
    }

    /// Threshold delivery has no entry and must not be requestable — it is not a
    /// charge, so a request for it is a misuse of the endpoint rather than a
    /// free line item.
    #[test]
    fn threshold_is_not_an_accessorial() {
        let err = price_accessorials(&php_card(), Currency::PHP, &[req("threshold", None)])
            .expect_err("threshold is free and has no config entry");
        assert_eq!(err, AccessorialError::NotOffered("threshold".into()));
    }

    /// The itemised rows and the total are the same calculation, so they cannot
    /// disagree. This is the carriage-fee invariant applied to accessorials.
    #[test]
    fn the_items_sum_to_the_total() {
        let requested = [
            req("helper", Some(3)),
            req("assembly", None),
            req("haulaway", None),
        ];
        let items = price_accessorials_itemised(&php_card(), Currency::PHP, &requested).unwrap();
        let total = price_accessorials(&php_card(), Currency::PHP, &requested).unwrap();
        let summed: i64 = items.iter().map(|i| i.billed_cents).sum();
        assert_eq!(summed, total.amount);
    }

    /// What the mover is paid is priced from its own column. A future discount
    /// moves billed; nothing here may derive paid from billed.
    #[test]
    fn paid_is_priced_from_its_own_column() {
        let items = price_accessorials_itemised(
            &php_card(),
            Currency::PHP,
            &[req("helper", Some(2)), req("haulaway", None)],
        )
        .unwrap();
        assert_eq!(items.iter().map(|i| i.billed_cents).sum::<i64>(), 2 * 81_200 + 226_200);
        assert_eq!(items.iter().map(|i| i.paid_cents).sum::<i64>(), 2 * 63_800 + 150_800);
    }

    /// paid_cents is settlement data. It reveals the take rate, so it must never
    /// reach the consumer app -- the quote breakdown serialises these items.
    #[test]
    fn a_priced_accessorial_never_serialises_paid() {
        let items =
            price_accessorials_itemised(&php_card(), Currency::PHP, &[req("assembly", None)]).unwrap();
        let json = serde_json::to_value(&items[0]).unwrap();
        assert!(json.get("paid_cents").is_none(), "settlement leaked: {json}");
        assert_eq!(json["amount_cents"], 150_800, "the wire name the design uses must stay");
    }

    /// A card paying the mover more than the customer is billed loses money on
    /// every job. That is a misconfiguration, and boot should refuse it.
    #[test]
    fn a_card_that_pays_more_than_it_bills_is_rejected() {
        assert!(php_card().validate().is_ok(), "the reference card must validate");
        let mut card = php_card();
        card.assembly.as_mut().unwrap().paid_cents = 999_999;
        let err = card.validate().expect_err("paid > billed must not boot");
        assert!(err.contains("assembly"), "error must name the code: {err}");
    }
}

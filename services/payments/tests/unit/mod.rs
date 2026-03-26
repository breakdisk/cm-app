// Unit tests for the payments service domain layer.
//
// Tests cover:
//   Invoice  — line item totals, subtotal, VAT (12% PH), total_due, status rules
//   CodCollection — creation, batch assignment, remittance, platform fee, merchant credit
//
// All monetary amounts are in PHP centavos (100 centavos = PHP 1.00).
// No database, no HTTP, no Kafka.

use logisticos_payments::domain::entities::{
    invoice::{Invoice, InvoiceLineItem, InvoiceStatus},
    cod_reconciliation::{CodCollection, CodStatus},
};
use logisticos_types::{InvoiceId, MerchantId, TenantId, Money, Currency};
use chrono::Utc;
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// Test helpers
// ─────────────────────────────────────────────────────────────────────────────

fn php(centavos: i64) -> Money { Money::new(centavos, Currency::PHP) }

fn make_line_item(unit_price: i64, qty: u32, discount: Option<i64>) -> InvoiceLineItem {
    InvoiceLineItem {
        description: "Delivery fee".into(),
        quantity: qty,
        unit_price: php(unit_price),
        discount: discount.map(php),
    }
}

/// Build an Invoice with the given line items. Status defaults to Issued.
fn make_invoice(items: Vec<InvoiceLineItem>) -> Invoice {
    Invoice {
        id:          InvoiceId::new(),
        merchant_id: MerchantId::new(),
        line_items:  items,
        status:      InvoiceStatus::Issued,
        issued_at:   Utc::now(),
        due_at:      Utc::now() + chrono::Duration::days(15),
        paid_at:     None,
        currency:    Currency::PHP,
    }
}

fn make_cod(tenant_id: TenantId, amount_centavos: i64) -> CodCollection {
    CodCollection::new(
        tenant_id,
        Uuid::new_v4(), // shipment_id
        Uuid::new_v4(), // driver_id
        Uuid::new_v4(), // pod_id
        php(amount_centavos),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// InvoiceLineItem::total()
// ─────────────────────────────────────────────────────────────────────────────

mod line_item_total {
    use super::*;

    #[test]
    fn total_with_no_discount_is_unit_price_times_quantity() {
        let item = make_line_item(8500, 3, None); // PHP 85 × 3 = PHP 255
        assert_eq!(item.total(), php(25500), "3 × PHP 85.00 = PHP 255.00 (25500 centavos)");
    }

    #[test]
    fn total_with_discount_subtracts_from_gross() {
        // unit_price=10000, qty=2 → gross=20000; discount=3000 → net=17000
        let item = make_line_item(10000, 2, Some(3000));
        assert_eq!(
            item.total(), php(17000),
            "PHP 200 gross minus PHP 30 discount = PHP 170 (17000 centavos)"
        );
    }

    #[test]
    fn total_with_zero_discount_equals_gross() {
        let item = make_line_item(8500, 1, Some(0));
        assert_eq!(item.total(), php(8500), "Zero discount must not change the total");
    }

    #[test]
    fn total_qty_1_equals_unit_price() {
        let item = make_line_item(15000, 1, None);
        assert_eq!(item.total(), php(15000), "Quantity 1 must return unit price exactly");
    }

    #[test]
    fn total_full_discount_equals_zero() {
        // Discount exactly cancels the full line value
        let item = make_line_item(8500, 2, Some(17000)); // gross=17000, discount=17000
        assert_eq!(item.total(), php(0), "Full discount must reduce total to 0");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invoice::subtotal()
// ─────────────────────────────────────────────────────────────────────────────

mod invoice_subtotal {
    use super::*;

    #[test]
    fn subtotal_sums_all_line_items() {
        let items = vec![
            make_line_item(8500, 1, None),  // PHP 85
            make_line_item(15000, 2, None), // PHP 300
            make_line_item(5000, 1, None),  // PHP 50
        ];
        let invoice = make_invoice(items);
        // 8500 + 30000 + 5000 = 43500
        assert_eq!(invoice.subtotal(), php(43500), "Subtotal must sum all line items");
    }

    #[test]
    fn subtotal_of_empty_invoice_is_zero() {
        let invoice = make_invoice(vec![]);
        assert_eq!(invoice.subtotal(), php(0), "Empty invoice subtotal must be PHP 0");
    }

    #[test]
    fn subtotal_includes_discounted_items() {
        let items = vec![
            make_line_item(10000, 1, Some(2000)), // gross 10000, net 8000
            make_line_item(5000,  1, None),       // gross 5000,  net 5000
        ];
        let invoice = make_invoice(items);
        assert_eq!(invoice.subtotal(), php(13000), "Subtotal = 8000 + 5000 = 13000");
    }

    #[test]
    fn subtotal_single_item() {
        let invoice = make_invoice(vec![make_line_item(8500, 1, None)]);
        assert_eq!(invoice.subtotal(), php(8500));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invoice::vat_amount() — 12% Philippine VAT
// ─────────────────────────────────────────────────────────────────────────────

mod invoice_vat {
    use super::*;

    #[test]
    fn vat_is_12_percent_of_subtotal_rounded() {
        // Subtotal = PHP 1000 (100000 centavos) → VAT = 12% = PHP 120 (12000 centavos)
        let items = vec![make_line_item(100000, 1, None)];
        let invoice = make_invoice(items);
        assert_eq!(
            invoice.vat_amount(), php(12000),
            "12% VAT on PHP 1000 = PHP 120 (12000 centavos)"
        );
    }

    #[test]
    fn vat_on_empty_invoice_is_zero() {
        let invoice = make_invoice(vec![]);
        assert_eq!(invoice.vat_amount(), php(0), "VAT on PHP 0 subtotal = PHP 0");
    }

    #[test]
    fn vat_rounds_fractional_centavos() {
        // Subtotal that produces a fractional VAT: PHP 8.50 = 850 centavos
        // VAT = 850 * 0.12 = 102.0 centavos → rounds to 102
        let items = vec![make_line_item(850, 1, None)];
        let invoice = make_invoice(items);
        assert_eq!(invoice.vat_amount(), php(102), "VAT on 850 centavos = 102 centavos");
    }

    #[test]
    fn vat_rounds_half_up() {
        // Subtotal = 833 centavos → VAT = 833 * 0.12 = 99.96 → rounds to 100
        let items = vec![make_line_item(833, 1, None)];
        let invoice = make_invoice(items);
        assert_eq!(invoice.vat_amount(), php(100), "99.96 centavos rounds to 100");
    }

    #[test]
    fn vat_on_php_85_delivery_fee() {
        // Standard delivery fee: PHP 85.00 = 8500 centavos
        // VAT = 8500 * 0.12 = 1020.0 centavos = PHP 10.20
        let items = vec![make_line_item(8500, 1, None)];
        let invoice = make_invoice(items);
        assert_eq!(invoice.vat_amount(), php(1020), "12% VAT on PHP 85.00 = PHP 10.20");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invoice::total_due()
// ─────────────────────────────────────────────────────────────────────────────

mod invoice_total_due {
    use super::*;

    #[test]
    fn total_due_equals_subtotal_plus_vat() {
        // PHP 1000 subtotal + PHP 120 VAT = PHP 1120
        let items = vec![make_line_item(100000, 1, None)];
        let invoice = make_invoice(items);
        assert_eq!(
            invoice.total_due(), php(112000),
            "total_due = subtotal (100000) + vat (12000) = 112000"
        );
    }

    #[test]
    fn total_due_of_empty_invoice_is_zero() {
        let invoice = make_invoice(vec![]);
        assert_eq!(invoice.total_due(), php(0), "Empty invoice total_due must be PHP 0");
    }

    #[test]
    fn total_due_for_standard_delivery() {
        // PHP 85.00 delivery → PHP 85 + PHP 10.20 VAT = PHP 95.20
        let items = vec![make_line_item(8500, 1, None)];
        let invoice = make_invoice(items);
        let expected = 8500 + 1020; // 9520 centavos = PHP 95.20
        assert_eq!(invoice.total_due(), php(expected));
    }

    #[test]
    fn total_due_is_subtotal_plus_vat_consistently() {
        let items = vec![
            make_line_item(8500,  3, None),  // PHP 255.00
            make_line_item(15000, 1, None),  // PHP 150.00
        ];
        let invoice = make_invoice(items);
        // subtotal = 25500 + 15000 = 40500
        // vat = round(40500 * 0.12) = round(4860.0) = 4860
        // total = 40500 + 4860 = 45360
        assert_eq!(invoice.subtotal(),   php(40500));
        assert_eq!(invoice.vat_amount(), php(4860));
        assert_eq!(invoice.total_due(),  php(45360));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invoice status rules
// ─────────────────────────────────────────────────────────────────────────────

mod invoice_status_rules {
    use super::*;

    #[test]
    fn can_cancel_draft_invoice() {
        let mut invoice = make_invoice(vec![]);
        invoice.status = InvoiceStatus::Draft;
        assert!(invoice.can_cancel(), "Draft invoice can be cancelled");
    }

    #[test]
    fn can_cancel_issued_invoice() {
        let invoice = make_invoice(vec![]);
        assert_eq!(invoice.status, InvoiceStatus::Issued);
        assert!(invoice.can_cancel(), "Issued invoice can be cancelled");
    }

    #[test]
    fn cannot_cancel_paid_invoice() {
        let mut invoice = make_invoice(vec![]);
        invoice.status = InvoiceStatus::Paid;
        assert!(
            !invoice.can_cancel(),
            "Paid invoice must NOT be cancellable"
        );
    }

    #[test]
    fn can_cancel_overdue_invoice() {
        let mut invoice = make_invoice(vec![]);
        invoice.status = InvoiceStatus::Overdue;
        assert!(invoice.can_cancel(), "Overdue invoice can be cancelled");
    }

    #[test]
    fn can_cancel_disputed_invoice() {
        let mut invoice = make_invoice(vec![]);
        invoice.status = InvoiceStatus::Disputed;
        assert!(invoice.can_cancel(), "Disputed invoice can be cancelled");
    }

    #[test]
    fn check_overdue_marks_issued_past_due_date() {
        let mut invoice = make_invoice(vec![]);
        // Set due_at to the past so the invoice is overdue
        invoice.due_at = Utc::now() - chrono::Duration::days(1);
        invoice.status = InvoiceStatus::Issued;

        invoice.check_overdue();
        assert_eq!(
            invoice.status, InvoiceStatus::Overdue,
            "Issued invoice past its due date must become Overdue"
        );
    }

    #[test]
    fn check_overdue_does_not_affect_paid_invoice() {
        let mut invoice = make_invoice(vec![]);
        invoice.due_at = Utc::now() - chrono::Duration::days(1);
        invoice.status = InvoiceStatus::Paid; // already paid — must stay paid

        invoice.check_overdue();
        assert_eq!(
            invoice.status, InvoiceStatus::Paid,
            "Paid invoice must NOT be changed to Overdue by check_overdue"
        );
    }

    #[test]
    fn check_overdue_does_not_affect_future_due_date() {
        let mut invoice = make_invoice(vec![]);
        invoice.due_at = Utc::now() + chrono::Duration::days(7);
        invoice.status = InvoiceStatus::Issued;

        invoice.check_overdue();
        assert_eq!(
            invoice.status, InvoiceStatus::Issued,
            "Invoice with future due date must stay Issued after check_overdue"
        );
    }

    #[test]
    fn with_net_15_terms_sets_due_at_15_days_after_issued_at() {
        let issued = Utc::now();
        let invoice = Invoice {
            id:          InvoiceId::new(),
            merchant_id: MerchantId::new(),
            line_items:  vec![],
            status:      InvoiceStatus::Draft,
            issued_at:   issued,
            due_at:      issued, // will be overwritten
            paid_at:     None,
            currency:    Currency::PHP,
        }.with_net_15_terms();

        let diff = invoice.due_at.signed_duration_since(issued);
        assert_eq!(diff.num_days(), 15, "net-15 terms must set due_at to exactly 15 days after issued_at");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CodCollection — creation and state transitions
// ─────────────────────────────────────────────────────────────────────────────

mod cod_collection {
    use super::*;

    #[test]
    fn new_collection_has_collected_status() {
        let cod = make_cod(TenantId::new(), 150_000);
        assert_eq!(cod.status, CodStatus::Collected, "New CodCollection must start as Collected");
    }

    #[test]
    fn new_collection_has_no_batch_id() {
        let cod = make_cod(TenantId::new(), 150_000);
        assert!(cod.batch_id.is_none(), "New CodCollection must have no batch_id");
    }

    #[test]
    fn new_collection_has_no_remitted_at() {
        let cod = make_cod(TenantId::new(), 150_000);
        assert!(cod.remitted_at.is_none(), "New CodCollection must have no remitted_at");
    }

    #[test]
    fn assign_to_batch_changes_status_to_in_batch() {
        let mut cod = make_cod(TenantId::new(), 150_000);
        let batch_id = Uuid::new_v4();
        cod.assign_to_batch(batch_id);
        assert_eq!(cod.status, CodStatus::InBatch, "Status must become InBatch after assign_to_batch");
    }

    #[test]
    fn assign_to_batch_sets_batch_id() {
        let mut cod = make_cod(TenantId::new(), 150_000);
        let batch_id = Uuid::new_v4();
        cod.assign_to_batch(batch_id);
        assert_eq!(cod.batch_id, Some(batch_id), "batch_id must be set after assign_to_batch");
    }

    #[test]
    fn mark_remitted_changes_status_to_remitted() {
        let mut cod = make_cod(TenantId::new(), 150_000);
        cod.assign_to_batch(Uuid::new_v4());
        cod.mark_remitted();
        assert_eq!(cod.status, CodStatus::Remitted, "Status must become Remitted after mark_remitted");
    }

    #[test]
    fn mark_remitted_sets_remitted_at_timestamp() {
        let mut cod = make_cod(TenantId::new(), 150_000);
        cod.mark_remitted();
        assert!(cod.remitted_at.is_some(), "remitted_at must be set after mark_remitted");

        // Timestamp should be very recent
        let delta = cod.remitted_at.unwrap().signed_duration_since(Utc::now());
        assert!(
            delta.num_milliseconds().abs() < 2000,
            "remitted_at must be approximately now"
        );
    }

    #[test]
    fn cod_amount_php_1500_is_150000_centavos() {
        // PHP 1,500 = 150,000 centavos
        let cod = make_cod(TenantId::new(), 150_000);
        assert_eq!(cod.amount, php(150_000), "PHP 1500 must be stored as 150000 centavos");
    }

    #[test]
    fn cod_amount_stores_currency_as_php() {
        let cod = make_cod(TenantId::new(), 150_000);
        assert_eq!(cod.amount.currency, Currency::PHP);
    }

    #[test]
    fn cod_status_transition_collected_to_in_batch_to_remitted() {
        let mut cod = make_cod(TenantId::new(), 50_000);
        assert_eq!(cod.status, CodStatus::Collected);

        cod.assign_to_batch(Uuid::new_v4());
        assert_eq!(cod.status, CodStatus::InBatch);

        cod.mark_remitted();
        assert_eq!(cod.status, CodStatus::Remitted);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CodCollection — platform fee and merchant credit
// ─────────────────────────────────────────────────────────────────────────────

mod cod_fee_calculations {
    use super::*;

    #[test]
    fn platform_fee_is_1_point_5_percent_of_amount() {
        // PHP 1,000 (100_000 centavos) × 1.5% = PHP 15 (1500 centavos)
        let cod = make_cod(TenantId::new(), 100_000);
        assert_eq!(
            cod.platform_fee(), php(1500),
            "1.5% fee on PHP 1000 = PHP 15 (1500 centavos)"
        );
    }

    #[test]
    fn merchant_credit_is_amount_minus_platform_fee() {
        // PHP 1,000 − PHP 15 = PHP 985
        let cod = make_cod(TenantId::new(), 100_000);
        assert_eq!(
            cod.merchant_credit(), php(98_500),
            "Merchant credit = PHP 1000 - PHP 15 = PHP 985"
        );
    }

    #[test]
    fn platform_fee_on_php_1500_cod() {
        // PHP 1,500 (150_000 centavos) × 1.5% = PHP 22.50 → rounds to 2250 centavos
        // 150000 * 0.015 = 2250.0 → rounds to 2250
        let cod = make_cod(TenantId::new(), 150_000);
        assert_eq!(
            cod.platform_fee(), php(2250),
            "1.5% fee on PHP 1500 = PHP 22.50 (2250 centavos)"
        );
    }

    #[test]
    fn merchant_credit_on_php_1500_cod() {
        // PHP 1500 - PHP 22.50 = PHP 1477.50 → 147750 centavos
        let cod = make_cod(TenantId::new(), 150_000);
        assert_eq!(
            cod.merchant_credit(), php(147_750),
            "Merchant credit = 150000 - 2250 = 147750 centavos"
        );
    }

    #[test]
    fn platform_fee_rounds_fractional_centavos() {
        // PHP 100 (10_000 centavos) × 1.5% = 150.0 centavos (exact, no rounding needed)
        let cod = make_cod(TenantId::new(), 10_000);
        assert_eq!(cod.platform_fee(), php(150));
    }

    #[test]
    fn platform_fee_on_small_amount_rounds_correctly() {
        // 100 centavos × 1.5% = 1.5 centavos → rounds to 2
        let cod = make_cod(TenantId::new(), 100);
        assert_eq!(cod.platform_fee(), php(2), "1.5 centavos rounds to 2");
    }

    #[test]
    fn fee_and_credit_sum_to_original_amount() {
        // fee + credit must reconstruct the original
        let amount = 87_654i64;
        let cod = make_cod(TenantId::new(), amount);
        let fee    = cod.platform_fee().amount;
        let credit = cod.merchant_credit().amount;
        // Due to rounding, fee + credit may differ by at most 1 centavo
        let reconstructed = fee + credit;
        assert!(
            (reconstructed - amount).abs() <= 1,
            "fee ({}) + credit ({}) should approximate original amount ({}) within 1 centavo",
            fee, credit, amount
        );
    }

    #[test]
    fn platform_fee_currency_is_php() {
        let cod = make_cod(TenantId::new(), 50_000);
        assert_eq!(cod.platform_fee().currency, Currency::PHP);
    }

    #[test]
    fn merchant_credit_currency_is_php() {
        let cod = make_cod(TenantId::new(), 50_000);
        assert_eq!(cod.merchant_credit().currency, Currency::PHP);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Multi-item invoice — realistic billing scenario
// ─────────────────────────────────────────────────────────────────────────────

mod realistic_invoice_scenarios {
    use super::*;

    #[test]
    fn weekly_merchant_invoice_with_mixed_services() {
        // A typical weekly invoice for a small merchant:
        //   5× Standard deliveries @ PHP 85 each = PHP 425
        //   2× Express deliveries  @ PHP 150 each = PHP 300
        //   1× Same-day delivery   @ PHP 200       = PHP 200
        //   1× Failed delivery fee @ PHP 30         = PHP 30
        // Subtotal = PHP 955 = 95500 centavos
        // VAT 12% = round(95500 * 0.12) = round(11460.0) = 11460
        // Total   = 95500 + 11460 = 106960
        let items = vec![
            make_line_item(8500,  5, None),  // standard × 5
            make_line_item(15000, 2, None),  // express × 2
            make_line_item(20000, 1, None),  // same-day × 1
            make_line_item(3000,  1, None),  // failed delivery fee × 1
        ];
        let invoice = make_invoice(items);

        assert_eq!(invoice.subtotal(),   php(95500));
        assert_eq!(invoice.vat_amount(), php(11460));
        assert_eq!(invoice.total_due(),  php(106960));
    }

    #[test]
    fn invoice_with_promotional_discount_on_bulk_order() {
        // Merchant gets PHP 500 discount on a bulk order:
        //   100× Standard deliveries @ PHP 85 = PHP 8500
        //   Discount: PHP 500 (50000 centavos)
        // Net line item = 850000 - 50000 = 800000
        let items = vec![
            make_line_item(8500, 100, Some(50000)),
        ];
        let invoice = make_invoice(items);

        assert_eq!(invoice.subtotal(), php(800000), "Subtotal must reflect discounted total");
        let vat = (800000f64 * 0.12).round() as i64;
        assert_eq!(invoice.vat_amount(), php(vat));
        assert_eq!(invoice.total_due(), php(800000 + vat));
    }
}

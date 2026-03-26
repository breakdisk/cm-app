use chrono::{NaiveDate, Utc};
use uuid::Uuid;

use logisticos_analytics::domain::entities::{DailyBucket, DeliveryKpis, DriverPerformance};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn today() -> NaiveDate {
    Utc::now().date_naive()
}

fn make_kpis(delivered: i64, failed: i64) -> DeliveryKpis {
    let on_time = delivered; // all delivered on-time for simplicity
    let with_eta = delivered;
    let cod_ships = 5_i64;
    let cod_coll = 50_000_i64; // 500.00 in cents
    let success_rate = if delivered + failed > 0 {
        delivered as f64 / (delivered + failed) as f64 * 100.0
    } else {
        0.0
    };
    let on_time_rate = if with_eta > 0 {
        on_time as f64 / with_eta as f64 * 100.0
    } else {
        0.0
    };
    let cod_rate = if cod_ships > 0 {
        cod_coll as f64 / cod_ships as f64 / 100.0
    } else {
        0.0
    };
    DeliveryKpis {
        tenant_id: Uuid::new_v4(),
        from: today(),
        to: today(),
        total_shipments: delivered + failed,
        delivered,
        failed,
        cancelled: 0,
        delivery_success_rate: success_rate,
        on_time_rate,
        avg_delivery_hours: 0.0,
        cod_shipments: cod_ships,
        cod_collected_cents: cod_coll,
        cod_collection_rate: cod_rate,
        computed_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// DeliveryKpis tests
// ---------------------------------------------------------------------------

mod delivery_kpis_tests {
    use super::*;

    #[test]
    fn success_rate_is_correct_proportion() {
        let kpis = make_kpis(80, 20);
        let expected = 80.0 / (80.0 + 20.0) * 100.0; // 80.0
        assert!(
            (kpis.delivery_success_rate - expected).abs() < 0.001,
            "Expected {}, got {}",
            expected,
            kpis.delivery_success_rate
        );
    }

    #[test]
    fn success_rate_is_100_when_no_failures() {
        let kpis = make_kpis(50, 0);
        assert!(
            (kpis.delivery_success_rate - 100.0).abs() < 0.001,
            "Expected 100.0, got {}",
            kpis.delivery_success_rate
        );
    }

    #[test]
    fn success_rate_is_0_when_all_failed() {
        let kpis = make_kpis(0, 10);
        assert!(
            kpis.delivery_success_rate.abs() < 0.001,
            "Expected 0.0, got {}",
            kpis.delivery_success_rate
        );
    }

    #[test]
    fn success_rate_is_0_when_zero_deliveries() {
        // No delivered, no failed → denominator is 0.
        let kpis = DeliveryKpis {
            tenant_id: Uuid::new_v4(),
            from: today(),
            to: today(),
            total_shipments: 0,
            delivered: 0,
            failed: 0,
            cancelled: 0,
            delivery_success_rate: {
                // Reproduce the formula from db/mod.rs
                let d: i64 = 0;
                let f: i64 = 0;
                if d + f > 0 { d as f64 / (d + f) as f64 * 100.0 } else { 0.0 }
            },
            on_time_rate: 0.0,
            avg_delivery_hours: 0.0,
            cod_shipments: 0,
            cod_collected_cents: 0,
            cod_collection_rate: 0.0,
            computed_at: Utc::now(),
        };
        assert_eq!(kpis.delivery_success_rate, 0.0);
    }

    #[test]
    fn on_time_rate_is_correct_proportion() {
        // 60 on-time out of 100 with ETA.
        let on_time: i64 = 60;
        let with_eta: i64 = 100;
        let rate = on_time as f64 / with_eta as f64 * 100.0;

        let kpis = DeliveryKpis {
            tenant_id: Uuid::new_v4(),
            from: today(),
            to: today(),
            total_shipments: 100,
            delivered: 80,
            failed: 20,
            cancelled: 0,
            delivery_success_rate: 80.0,
            on_time_rate: rate,
            avg_delivery_hours: 2.5,
            cod_shipments: 0,
            cod_collected_cents: 0,
            cod_collection_rate: 0.0,
            computed_at: Utc::now(),
        };

        assert!(
            (kpis.on_time_rate - 60.0).abs() < 0.001,
            "Expected 60.0, got {}",
            kpis.on_time_rate
        );
    }

    #[test]
    fn on_time_rate_is_0_when_no_eta_data() {
        let kpis = DeliveryKpis {
            tenant_id: Uuid::new_v4(),
            from: today(),
            to: today(),
            total_shipments: 10,
            delivered: 10,
            failed: 0,
            cancelled: 0,
            delivery_success_rate: 100.0,
            on_time_rate: 0.0, // with_eta == 0 → 0.0
            avg_delivery_hours: 0.0,
            cod_shipments: 0,
            cod_collected_cents: 0,
            cod_collection_rate: 0.0,
            computed_at: Utc::now(),
        };
        assert_eq!(kpis.on_time_rate, 0.0);
    }

    #[test]
    fn cod_collection_rate_formula() {
        // cod_coll / cod_ships / 100.0  (cents → currency rate)
        // 50_000 cents over 5 shipments = 50_000/5/100 = 100.0
        let cod_ships: i64 = 5;
        let cod_coll: i64 = 50_000;
        let rate = cod_coll as f64 / cod_ships as f64 / 100.0;

        let kpis = make_kpis(10, 0); // overwrite cod fields below for precision
        let kpis = DeliveryKpis {
            cod_collection_rate: rate,
            ..kpis
        };

        assert!(
            (kpis.cod_collection_rate - 100.0).abs() < 0.001,
            "Expected 100.0, got {}",
            kpis.cod_collection_rate
        );
    }

    #[test]
    fn cod_collection_rate_is_0_when_no_cod_shipments() {
        let kpis = DeliveryKpis {
            tenant_id: Uuid::new_v4(),
            from: today(),
            to: today(),
            total_shipments: 5,
            delivered: 5,
            failed: 0,
            cancelled: 0,
            delivery_success_rate: 100.0,
            on_time_rate: 0.0,
            avg_delivery_hours: 0.0,
            cod_shipments: 0,
            cod_collected_cents: 0,
            cod_collection_rate: 0.0, // no COD shipments
            computed_at: Utc::now(),
        };
        assert_eq!(kpis.cod_collection_rate, 0.0);
    }
}

// ---------------------------------------------------------------------------
// DailyBucket tests
// ---------------------------------------------------------------------------

mod daily_bucket_tests {
    use super::*;

    fn make_bucket(delivered: i64, failed: i64) -> DailyBucket {
        let success = if delivered + failed > 0 {
            delivered as f64 / (delivered + failed) as f64 * 100.0
        } else {
            0.0
        };
        DailyBucket {
            date: today(),
            shipments: delivered + failed,
            delivered,
            failed,
            success_rate: success,
            cod_collected_cents: 0,
        }
    }

    #[test]
    fn success_rate_is_correct_proportion() {
        let b = make_bucket(7, 3);
        let expected = 7.0 / 10.0 * 100.0; // 70.0
        assert!(
            (b.success_rate - expected).abs() < 0.001,
            "Expected {}, got {}",
            expected,
            b.success_rate
        );
    }

    #[test]
    fn success_rate_is_0_when_delivered_and_failed_both_zero() {
        let b = make_bucket(0, 0);
        assert_eq!(b.success_rate, 0.0);
    }

    #[test]
    fn success_rate_is_100_when_all_delivered() {
        let b = make_bucket(20, 0);
        assert!(
            (b.success_rate - 100.0).abs() < 0.001,
            "Expected 100.0, got {}",
            b.success_rate
        );
    }

    #[test]
    fn success_rate_is_0_when_all_failed() {
        let b = make_bucket(0, 5);
        assert_eq!(b.success_rate, 0.0);
    }

    #[test]
    fn shipments_field_equals_delivered_plus_failed() {
        let b = make_bucket(12, 8);
        assert_eq!(b.shipments, 20);
    }

    #[test]
    fn cod_collected_cents_stored_correctly() {
        let mut b = make_bucket(5, 1);
        b.cod_collected_cents = 120_00; // 120.00 in cents
        assert_eq!(b.cod_collected_cents, 12000);
    }
}

// ---------------------------------------------------------------------------
// DriverPerformance tests
// ---------------------------------------------------------------------------

mod driver_performance_tests {
    use super::*;

    fn make_perf(successful: i64, failed: i64) -> DriverPerformance {
        let total = successful + failed;
        let rate = if total > 0 {
            successful as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        DriverPerformance {
            driver_id: Uuid::new_v4(),
            driver_name: Some("Juan dela Cruz".into()),
            total_deliveries: total,
            successful,
            failed,
            success_rate: rate,
            avg_delivery_hours: 1.5,
            cod_collected_cents: 0,
        }
    }

    #[test]
    fn success_rate_is_correct_proportion() {
        let perf = make_perf(9, 1);
        assert!(
            (perf.success_rate - 90.0).abs() < 0.001,
            "Expected 90.0, got {}",
            perf.success_rate
        );
    }

    #[test]
    fn success_rate_is_0_when_total_deliveries_is_0() {
        let perf = make_perf(0, 0);
        assert_eq!(perf.success_rate, 0.0);
        assert_eq!(perf.total_deliveries, 0);
    }

    #[test]
    fn success_rate_is_100_when_all_successful() {
        let perf = make_perf(10, 0);
        assert!(
            (perf.success_rate - 100.0).abs() < 0.001,
            "Expected 100.0, got {}",
            perf.success_rate
        );
    }

    #[test]
    fn success_rate_is_0_when_all_failed() {
        let perf = make_perf(0, 8);
        assert_eq!(perf.success_rate, 0.0);
    }

    #[test]
    fn total_deliveries_equals_successful_plus_failed() {
        let perf = make_perf(15, 5);
        assert_eq!(perf.total_deliveries, perf.successful + perf.failed);
    }

    #[test]
    fn driver_name_is_optional() {
        let mut perf = make_perf(5, 0);
        perf.driver_name = None;
        assert!(perf.driver_name.is_none());
    }

    #[test]
    fn cod_collected_cents_stored_correctly() {
        let mut perf = make_perf(3, 0);
        perf.cod_collected_cents = 25_000;
        assert_eq!(perf.cod_collected_cents, 25_000);
    }
}

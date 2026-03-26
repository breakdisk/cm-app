// Integration tests for the logisticos-geo crate.
//
// Uses real Philippine city coordinates to validate haversine distance,
// nearest-neighbor ordering, bounding-box operations, zone assignment,
// coordinate validation, and the ph:: hub constants.

use logisticos_geo::{
    haversine_km, nearest_neighbor_order, assign_zone,
    BoundingBox, Coordinates, DeliveryZone, ph,
};

// ─────────────────────────────────────────────────────────────────────────────
// Test helpers
// ─────────────────────────────────────────────────────────────────────────────

fn make_zone(id: &str, min_lat: f64, min_lng: f64, max_lat: f64, max_lng: f64) -> DeliveryZone {
    DeliveryZone {
        id:     id.into(),
        name:   id.into(),
        bounds: BoundingBox::new(min_lat, min_lng, max_lat, max_lng),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Haversine distance — real Philippine city pairs
// ─────────────────────────────────────────────────────────────────────────────

mod haversine_distances {
    use super::*;

    #[test]
    fn manila_to_cebu_is_approximately_565_km() {
        let d = haversine_km(&ph::MANILA, &ph::CEBU);
        // Actual great-circle: ~565 km. Allow ±20 km for coordinate precision.
        assert!(
            (d - 565.0).abs() < 20.0,
            "Manila–Cebu haversine = {:.1} km (expected ~565 km)",
            d
        );
    }

    #[test]
    fn manila_to_davao_is_approximately_970_km() {
        let d = haversine_km(&ph::MANILA, &ph::DAVAO);
        // Great-circle: ~970 km. Allow ±25 km.
        assert!(
            (d - 970.0).abs() < 25.0,
            "Manila–Davao haversine = {:.1} km (expected ~970 km)",
            d
        );
    }

    #[test]
    fn same_point_distance_is_zero() {
        let d = haversine_km(&ph::MANILA, &ph::MANILA);
        assert!(
            d < 0.001,
            "Distance from a point to itself must be ~0 km, got {:.6}",
            d
        );
    }

    #[test]
    fn distance_is_symmetric() {
        let d_ab = haversine_km(&ph::MANILA, &ph::CEBU);
        let d_ba = haversine_km(&ph::CEBU, &ph::MANILA);
        assert!(
            (d_ab - d_ba).abs() < 0.0001,
            "Haversine must be symmetric: d(A,B)={:.4} d(B,A)={:.4}",
            d_ab, d_ba
        );
    }

    #[test]
    fn cebu_to_davao_is_less_than_manila_to_davao() {
        // Cebu is geographically between Manila and Davao
        let manila_davao = haversine_km(&ph::MANILA, &ph::DAVAO);
        let cebu_davao   = haversine_km(&ph::CEBU,   &ph::DAVAO);
        assert!(
            cebu_davao < manila_davao,
            "Cebu–Davao ({:.1} km) should be shorter than Manila–Davao ({:.1} km)",
            cebu_davao, manila_davao
        );
    }

    #[test]
    fn driving_distance_applies_road_factor_of_1_point_3() {
        // Coordinates::driving_distance_km = haversine * 1.3
        let a = ph::MANILA;
        let b = ph::CEBU;
        let straight = a.distance_km(&b);
        let driving  = a.driving_distance_km(&b);
        let ratio    = driving / straight;
        assert!(
            (ratio - 1.3).abs() < 0.0001,
            "driving_distance_km must be exactly haversine × 1.3, got ratio={:.4}",
            ratio
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Nearest-neighbor ordering — Metro Manila delivery scenario
// ─────────────────────────────────────────────────────────────────────────────

mod nearest_neighbor_metro_manila {
    use super::*;

    // Depot / origin: Makati
    const MAKATI: Coordinates = Coordinates { lat: 14.5547, lng: 121.0244 };

    // Five delivery stops in Metro Manila with known relative distances from Makati
    // A: Pasig       (14.5764, 121.0851) — east,  ~6 km
    // B: Mandaluyong (14.5794, 121.0359) — north, ~3 km (closest to Makati)
    // C: Marikina    (14.6507, 121.1029) — far north-east, ~12 km
    // D: Taguig      (14.5176, 121.0509) — south-east, ~5 km
    // E: Paranaque   (14.4793, 121.0198) — far south, ~9 km
    const PASIG:        Coordinates = Coordinates { lat: 14.5764, lng: 121.0851 };
    const MANDALUYONG:  Coordinates = Coordinates { lat: 14.5794, lng: 121.0359 };
    const MARIKINA:     Coordinates = Coordinates { lat: 14.6507, lng: 121.1029 };
    const TAGUIG:       Coordinates = Coordinates { lat: 14.5176, lng: 121.0509 };
    const PARANAQUE:    Coordinates = Coordinates { lat: 14.4793, lng: 121.0198 };

    #[test]
    fn mandaluyong_is_the_first_stop_from_makati() {
        // Mandaluyong is the closest stop to Makati (~3 km).
        let stops = vec![
            PASIG,       // index 0
            MANDALUYONG, // index 1 — nearest
            MARIKINA,    // index 2
            TAGUIG,      // index 3
            PARANAQUE,   // index 4
        ];

        let order = nearest_neighbor_order(&MAKATI, &stops);
        assert_eq!(order.len(), 5, "All 5 stops must appear in the ordering");
        assert_eq!(
            order[0], 1,
            "Mandaluyong (index 1, ~3 km from Makati) must be visited first. \
             Order was: {:?}", order
        );
    }

    #[test]
    fn paranaque_is_not_the_first_stop_from_makati() {
        let stops = vec![
            PASIG, MANDALUYONG, MARIKINA, TAGUIG, PARANAQUE,
        ];
        let order = nearest_neighbor_order(&MAKATI, &stops);
        assert_ne!(
            order[0], 4,
            "Paranaque (index 4, ~9 km) must NOT be visited first from Makati"
        );
    }

    #[test]
    fn all_five_stops_appear_exactly_once() {
        let stops = vec![PASIG, MANDALUYONG, MARIKINA, TAGUIG, PARANAQUE];
        let order = nearest_neighbor_order(&MAKATI, &stops);

        let mut seen = vec![false; 5];
        for idx in &order {
            assert!(!seen[*idx], "Stop index {} appeared more than once", idx);
            seen[*idx] = true;
        }
        assert!(seen.iter().all(|&s| s), "Not all stops were visited: {:?}", order);
    }

    #[test]
    fn first_stop_is_closer_to_origin_than_last_stop() {
        let stops = vec![PASIG, MANDALUYONG, MARIKINA, TAGUIG, PARANAQUE];
        let order = nearest_neighbor_order(&MAKATI, &stops);

        let first_dist = MAKATI.distance_km(&stops[order[0]]);
        let last_dist  = MAKATI.distance_km(&stops[order[order.len() - 1]]);

        assert!(
            first_dist < last_dist,
            "First stop ({:.2} km) must be closer to Makati than last stop ({:.2} km)",
            first_dist, last_dist
        );
    }

    #[test]
    fn mandaluyong_distance_from_makati_is_less_than_3_point_5_km() {
        let dist = MAKATI.distance_km(&MANDALUYONG);
        assert!(
            dist < 3.5,
            "Mandaluyong should be within 3.5 km of Makati, got {:.2} km",
            dist
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BoundingBox
// ─────────────────────────────────────────────────────────────────────────────

mod bounding_box {
    use super::*;

    #[test]
    fn from_center_contains_the_center_point() {
        let center = ph::MANILA;
        let bb = BoundingBox::from_center(&center, 10.0);
        assert!(
            bb.contains(&center),
            "BoundingBox::from_center must contain the center point itself"
        );
    }

    #[test]
    fn from_center_10km_does_not_contain_cebu() {
        // Manila–Cebu is ~565 km — way outside a 10 km bounding box from Manila
        let bb = BoundingBox::from_center(&ph::MANILA, 10.0);
        assert!(
            !bb.contains(&ph::CEBU),
            "A 10 km bounding box around Manila must NOT contain Cebu"
        );
    }

    #[test]
    fn bounding_box_center_roundtrips_correctly() {
        let original = Coordinates::new(14.5995, 120.9842);
        let bb = BoundingBox::from_center(&original, 5.0);
        let computed_center = bb.center();

        assert!(
            (computed_center.lat - original.lat).abs() < 0.0001,
            "Center lat mismatch: {:.6} vs {:.6}",
            computed_center.lat, original.lat
        );
        assert!(
            (computed_center.lng - original.lng).abs() < 0.0001,
            "Center lng mismatch: {:.6} vs {:.6}",
            computed_center.lng, original.lng
        );
    }

    #[test]
    fn contains_point_on_boundary_returns_true() {
        let bb = BoundingBox::new(14.0, 120.0, 15.0, 121.0);
        // Corners
        assert!(bb.contains(&Coordinates::new(14.0, 120.0)), "min corner");
        assert!(bb.contains(&Coordinates::new(15.0, 121.0)), "max corner");
        assert!(bb.contains(&Coordinates::new(14.0, 121.0)), "min_lat, max_lng corner");
        assert!(bb.contains(&Coordinates::new(15.0, 120.0)), "max_lat, min_lng corner");
    }

    #[test]
    fn does_not_contain_point_just_outside() {
        let bb = BoundingBox::new(14.0, 120.0, 15.0, 121.0);
        assert!(!bb.contains(&Coordinates::new(15.0001, 120.5)), "lat just over max");
        assert!(!bb.contains(&Coordinates::new(14.5, 121.0001)), "lng just over max");
        assert!(!bb.contains(&Coordinates::new(13.9999, 120.5)), "lat just under min");
    }

    #[test]
    fn from_center_larger_radius_contains_more_points() {
        let center = ph::MANILA;
        let small_bb = BoundingBox::from_center(&center, 5.0);
        let large_bb = BoundingBox::from_center(&center, 50.0);

        // A point roughly 40 km from Manila
        let distant_point = Coordinates::new(14.96, 120.98); // ~40 km north
        assert!(!small_bb.contains(&distant_point),  "5 km box must not contain a 40 km distant point");
        assert!( large_bb.contains(&distant_point),  "50 km box must contain a 40 km distant point");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Zone assignment
// ─────────────────────────────────────────────────────────────────────────────

mod zone_assignment {
    use super::*;

    fn ph_zones() -> Vec<DeliveryZone> {
        vec![
            // NCR bounding box (approximate)
            make_zone("NCR", 14.35, 120.90, 14.78, 121.12),
            // CALABARZON — south of NCR
            make_zone("CALABARZON", 13.50, 120.50, 14.34, 122.00),
            // Cebu province
            make_zone("CEBU", 9.50, 123.00, 11.50, 124.50),
        ]
    }

    #[test]
    fn manila_is_assigned_to_ncr() {
        let zones = ph_zones();
        let zone = assign_zone(&ph::MANILA, &zones);
        assert!(zone.is_some(), "Manila must match a zone");
        assert_eq!(zone.unwrap().id, "NCR", "Manila must be in NCR");
    }

    #[test]
    fn cebu_city_is_assigned_to_cebu_zone() {
        let zones = ph_zones();
        let zone = assign_zone(&ph::CEBU, &zones);
        assert!(zone.is_some(), "Cebu must match a zone");
        assert_eq!(zone.unwrap().id, "CEBU", "Cebu city must be in CEBU zone");
    }

    #[test]
    fn davao_matches_no_defined_zone() {
        let zones = ph_zones(); // no Mindanao zone defined
        let zone = assign_zone(&ph::DAVAO, &zones);
        assert!(
            zone.is_none(),
            "Davao must return None when no Mindanao zone is configured"
        );
    }

    #[test]
    fn assign_zone_returns_first_matching_zone() {
        // Overlapping zones — ensure first match (NCR) is returned
        let zones = vec![
            make_zone("NCR",  14.35, 120.90, 14.78, 121.12),
            make_zone("WIDE", 10.00, 118.00, 20.00, 126.00), // covers all PH
        ];
        let zone = assign_zone(&ph::MANILA, &zones).unwrap(); // safe: Manila is in both zones
        assert_eq!(zone.id, "NCR", "First matching zone must be returned");
    }

    #[test]
    fn assign_zone_returns_none_for_empty_zone_list() {
        let zones: Vec<DeliveryZone> = vec![];
        assert!(assign_zone(&ph::MANILA, &zones).is_none());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Coordinates validation
// ─────────────────────────────────────────────────────────────────────────────

mod coordinates_validation {
    use super::*;

    #[test]
    fn valid_philippine_coordinates_pass_validation() {
        assert!(ph::MANILA.is_valid(),    "Manila must be valid");
        assert!(ph::CEBU.is_valid(),      "Cebu must be valid");
        assert!(ph::DAVAO.is_valid(),     "Davao must be valid");
        assert!(ph::ILOILO.is_valid(),    "Iloilo must be valid");
        assert!(ph::CAGAYAN.is_valid(),   "Cagayan must be valid");
        assert!(ph::ZAMBOANGA.is_valid(), "Zamboanga must be valid");
    }

    #[test]
    fn latitude_above_90_is_invalid() {
        let bad = Coordinates::new(91.0, 120.0);
        assert!(!bad.is_valid(), "lat=91 must be invalid");
    }

    #[test]
    fn latitude_below_minus_90_is_invalid() {
        let bad = Coordinates::new(-91.0, 120.0);
        assert!(!bad.is_valid(), "lat=-91 must be invalid");
    }

    #[test]
    fn longitude_above_180_is_invalid() {
        let bad = Coordinates::new(14.5, 181.0);
        assert!(!bad.is_valid(), "lng=181 must be invalid");
    }

    #[test]
    fn longitude_below_minus_180_is_invalid() {
        let bad = Coordinates::new(14.5, -181.0);
        assert!(!bad.is_valid(), "lng=-181 must be invalid");
    }

    #[test]
    fn boundary_coordinates_are_valid() {
        assert!(Coordinates::new(90.0, 180.0).is_valid(),    "Max boundaries must be valid");
        assert!(Coordinates::new(-90.0, -180.0).is_valid(), "Min boundaries must be valid");
        assert!(Coordinates::new(0.0, 0.0).is_valid(),       "Null Island must be valid");
    }

    #[test]
    fn wkt_string_has_correct_format() {
        let c = Coordinates::new(14.5995, 120.9842);
        let wkt = c.wkt();
        // WKT POINT uses (lng lat) order per OGC standard
        assert!(wkt.starts_with("POINT("), "WKT must start with POINT(");
        assert!(wkt.contains("120.9842"), "WKT must contain longitude");
        assert!(wkt.contains("14.5995"),  "WKT must contain latitude");
    }

    #[test]
    fn display_format_is_lat_comma_lng() {
        let c = Coordinates::new(14.5995, 120.9842);
        let s = format!("{}", c);
        assert_eq!(s, "14.5995,120.9842", "Display must be 'lat,lng'");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ph:: constants — sanity checks
// ─────────────────────────────────────────────────────────────────────────────

mod ph_constants {
    use super::*;

    #[test]
    fn all_ph_hubs_are_valid_coordinates() {
        let hubs = [ph::MANILA, ph::CEBU, ph::DAVAO, ph::ILOILO, ph::CAGAYAN, ph::ZAMBOANGA];
        for hub in &hubs {
            assert!(hub.is_valid(), "Hub {:?} must have valid coordinates", hub);
        }
    }

    #[test]
    fn all_ph_hubs_are_within_philippine_lat_lng_bounds() {
        // Philippine archipelago: lat 4°–22°N, lng 116°–128°E
        let hubs = [ph::MANILA, ph::CEBU, ph::DAVAO, ph::ILOILO, ph::CAGAYAN, ph::ZAMBOANGA];
        for hub in &hubs {
            assert!(
                hub.lat >= 4.0 && hub.lat <= 22.0,
                "Hub lat {:.4} is outside PH bounds (4–22°N)", hub.lat
            );
            assert!(
                hub.lng >= 116.0 && hub.lng <= 128.0,
                "Hub lng {:.4} is outside PH bounds (116–128°E)", hub.lng
            );
        }
    }

    #[test]
    fn manila_is_north_of_cebu() {
        assert!(
            ph::MANILA.lat > ph::CEBU.lat,
            "Manila (lat={}) must be north of Cebu (lat={})",
            ph::MANILA.lat, ph::CEBU.lat
        );
    }

    #[test]
    fn davao_is_the_southernmost_hub() {
        let hubs = [ph::MANILA, ph::CEBU, ph::ILOILO, ph::CAGAYAN, ph::ZAMBOANGA];
        for hub in &hubs {
            assert!(
                ph::DAVAO.lat < hub.lat,
                "Davao (lat={:.4}) must be south of {:?} (lat={:.4})",
                ph::DAVAO.lat, hub, hub.lat
            );
        }
    }

    #[test]
    fn manila_hub_coordinates_match_known_values() {
        assert_eq!(ph::MANILA.lat, 14.5995);
        assert_eq!(ph::MANILA.lng, 120.9842);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// nearest_neighbor_order — edge cases and correctness
// ─────────────────────────────────────────────────────────────────────────────

mod nearest_neighbor_edge_cases {
    use super::*;

    #[test]
    fn identical_stops_return_stable_ordering() {
        let same = Coordinates::new(14.60, 121.00);
        let stops = vec![same, same, same];
        let order = nearest_neighbor_order(&ph::MANILA, &stops);
        assert_eq!(order.len(), 3, "Three identical stops must all be visited");
        // All three indices must appear
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![0, 1, 2]);
    }

    #[test]
    fn two_stops_are_ordered_nearest_first() {
        let near = Coordinates::new(14.61, 120.99); // ~2 km from Manila
        let far  = Coordinates::new(14.90, 121.30); // ~40 km from Manila

        let stops = vec![far, near]; // far at 0, near at 1
        let order = nearest_neighbor_order(&ph::MANILA, &stops);

        assert_eq!(order[0], 1, "Near stop (index 1) must be visited before far stop (index 0)");
    }

    #[test]
    fn large_stop_count_returns_all_indices() {
        let n = 50;
        let stops: Vec<Coordinates> = (0..n)
            .map(|i| Coordinates::new(14.0 + i as f64 * 0.01, 121.0))
            .collect();

        let order = nearest_neighbor_order(&ph::MANILA, &stops);
        assert_eq!(order.len(), n, "All {n} stops must be present in the order");

        let mut sorted = order.clone();
        sorted.sort_unstable();
        let expected: Vec<usize> = (0..n).collect();
        assert_eq!(sorted, expected, "Each index from 0 to {n}-1 must appear exactly once");
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxItem {
    pub awb:        String,
    pub weight_g:   u32,
    pub length_cm:  u32,
    pub width_cm:   u32,
    pub height_cm:  u32,
    pub estimated:  bool,
}

#[derive(Debug, Clone)]
pub struct Bin {
    pub length_cm:    u32,
    pub width_cm:     u32,
    pub height_cm:    u32,
    pub max_payload_g: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Placement {
    pub awb:        String,
    pub x:          u32,
    pub y:          u32,
    pub z:          u32,
    pub length_cm:  u32,
    pub width_cm:   u32,
    pub height_cm:  u32,
    pub weight_g:   u32,
    pub estimated:  bool,
    pub rotated:    bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnplacedItem {
    pub awb:      String,
    pub weight_g: u32,
    pub reason:   String,
}

#[derive(Debug, Clone)]
pub struct PackResult {
    pub placements:       Vec<Placement>,
    pub unplaced:         Vec<UnplacedItem>,
    pub volume_used_cm3:  u64,
    pub volume_total_cm3: u64,
    pub total_weight_kg:  f64,
}

// An extreme point candidate for item placement.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ExtremePoint {
    x: u32,
    y: u32,
    z: u32,
}

impl ExtremePoint {
    // Prefer lowest z (floor first), then lowest x, then lowest y — standard EP heuristic.
    fn sort_key(&self) -> (u32, u32, u32) {
        (self.z, self.x, self.y)
    }
}

pub fn pack(bin: &Bin, items: &[BoxItem]) -> PackResult {
    let volume_total_cm3 = bin.length_cm as u64 * bin.width_cm as u64 * bin.height_cm as u64;

    // Sort items descending by volume so larger items anchor placement early.
    let mut sorted: Vec<&BoxItem> = items.iter().collect();
    sorted.sort_by(|a, b| {
        let va = a.length_cm as u64 * a.width_cm as u64 * a.height_cm as u64;
        let vb = b.length_cm as u64 * b.width_cm as u64 * b.height_cm as u64;
        vb.cmp(&va)
    });

    let mut placements: Vec<Placement> = Vec::new();
    let mut unplaced:   Vec<UnplacedItem> = Vec::new();
    let mut total_weight_g: u64 = 0;
    let mut volume_used_cm3: u64 = 0;

    // Seed the extreme point list at the origin.
    let mut eps: Vec<ExtremePoint> = vec![ExtremePoint { x: 0, y: 0, z: 0 }];

    for item in sorted {
        if item.length_cm == 0 || item.width_cm == 0 || item.height_cm == 0 {
            unplaced.push(UnplacedItem {
                awb:      item.awb.clone(),
                weight_g: item.weight_g,
                reason:   "zero_dimension".into(),
            });
            continue;
        }

        if total_weight_g + item.weight_g as u64 > bin.max_payload_g {
            unplaced.push(UnplacedItem {
                awb:      item.awb.clone(),
                weight_g: item.weight_g,
                reason:   "weight_limit".into(),
            });
            continue;
        }

        // Two orientations: original and 90° yaw (swap L and W, H fixed).
        let orientations = [
            (item.length_cm, item.width_cm, item.height_cm, false),
            (item.width_cm,  item.length_cm, item.height_cm, true),
        ];

        // Sort EPs by the heuristic key before searching.
        eps.sort_by_key(|ep| ep.sort_key());
        eps.dedup_by(|a, b| a == b);

        let mut placed = false;
        // Collect new extreme points outside the loop to satisfy the borrow checker:
        // we cannot push to `eps` while `&eps` is borrowed by the for-loop iterator.
        let mut pending_eps: Vec<ExtremePoint> = Vec::new();

        'ep_loop: for ep in &eps {
            for &(l, w, h, rotated) in &orientations {
                if ep.x + l > bin.length_cm
                    || ep.y + w > bin.width_cm
                    || ep.z + h > bin.height_cm
                {
                    continue;
                }
                if overlaps_any(&placements, ep.x, ep.y, ep.z, l, w, h) {
                    continue;
                }

                // Found a valid placement.
                total_weight_g  += item.weight_g as u64;
                volume_used_cm3 += l as u64 * w as u64 * h as u64;

                // Stage the three new EPs derived from this box's faces.
                // Bounds + duplicate checks are deferred to after the loop.
                pending_eps.push(ExtremePoint { x: ep.x + l, y: ep.y,     z: ep.z     });
                pending_eps.push(ExtremePoint { x: ep.x,     y: ep.y + w,  z: ep.z     });
                pending_eps.push(ExtremePoint { x: ep.x,     y: ep.y,      z: ep.z + h });

                placements.push(Placement {
                    awb:       item.awb.clone(),
                    x:         ep.x,
                    y:         ep.y,
                    z:         ep.z,
                    length_cm: l,
                    width_cm:  w,
                    height_cm: h,
                    weight_g:  item.weight_g,
                    estimated: item.estimated,
                    rotated,
                });
                placed = true;
                break 'ep_loop;
            }
        }

        // Push staged EPs now that the immutable borrow on `eps` is released.
        for new_ep in pending_eps {
            if new_ep.x <= bin.length_cm
                && new_ep.y <= bin.width_cm
                && new_ep.z <= bin.height_cm
                && !eps.contains(&new_ep)
            {
                eps.push(new_ep);
            }
        }

        if !placed {
            unplaced.push(UnplacedItem {
                awb:      item.awb.clone(),
                weight_g: item.weight_g,
                reason:   "no_space".into(),
            });
        }
    }

    PackResult {
        placements,
        unplaced,
        volume_used_cm3,
        volume_total_cm3,
        total_weight_kg: total_weight_g as f64 / 1000.0,
    }
}

/// Returns true if the candidate box (px, py, pz, pl, pw, ph) overlaps any existing placement.
fn overlaps_any(placements: &[Placement], px: u32, py: u32, pz: u32, pl: u32, pw: u32, ph: u32) -> bool {
    for p in placements {
        if px < p.x + p.length_cm
            && px + pl > p.x
            && py < p.y + p.width_cm
            && py + pw > p.y
            && pz < p.z + p.height_cm
            && pz + ph > p.z
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bin_l300() -> Bin {
        Bin { length_cm: 280, width_cm: 170, height_cm: 140, max_payload_g: 1_500_000 }
    }

    #[test]
    fn single_item_fits_at_origin() {
        let item = BoxItem { awb: "TEST-001".into(), weight_g: 1000, length_cm: 50, width_cm: 40, height_cm: 30, estimated: false };
        let result = pack(&bin_l300(), &[item]);
        assert_eq!(result.placements.len(), 1);
        assert!(result.unplaced.is_empty());
        let p = &result.placements[0];
        assert_eq!((p.x, p.y, p.z), (0, 0, 0));
    }

    #[test]
    fn oversized_item_goes_to_unplaced() {
        let item = BoxItem { awb: "BIG-001".into(), weight_g: 500, length_cm: 999, width_cm: 40, height_cm: 30, estimated: false };
        let result = pack(&bin_l300(), &[item]);
        assert!(result.placements.is_empty());
        assert_eq!(result.unplaced[0].reason, "no_space");
    }

    #[test]
    fn weight_limit_exceeded() {
        let bin = Bin { length_cm: 280, width_cm: 170, height_cm: 140, max_payload_g: 100 };
        let item = BoxItem { awb: "HEAVY-001".into(), weight_g: 500, length_cm: 10, width_cm: 10, height_cm: 10, estimated: false };
        let result = pack(&bin, &[item]);
        assert!(result.placements.is_empty());
        assert_eq!(result.unplaced[0].reason, "weight_limit");
    }

    #[test]
    fn two_items_stack_vertically() {
        // Each box exactly half the bin height — should both fit.
        let items = vec![
            BoxItem { awb: "A".into(), weight_g: 100, length_cm: 10, width_cm: 10, height_cm: 70, estimated: false },
            BoxItem { awb: "B".into(), weight_g: 100, length_cm: 10, width_cm: 10, height_cm: 70, estimated: false },
        ];
        let result = pack(&bin_l300(), &items);
        assert_eq!(result.placements.len(), 2);
        assert!(result.unplaced.is_empty());
    }

    #[test]
    fn volume_stats_are_correct() {
        let item = BoxItem { awb: "V".into(), weight_g: 100, length_cm: 10, width_cm: 10, height_cm: 10, estimated: false };
        let result = pack(&bin_l300(), &[item]);
        assert_eq!(result.volume_used_cm3, 1000);
        assert_eq!(result.volume_total_cm3, 280 * 170 * 140);
    }
}

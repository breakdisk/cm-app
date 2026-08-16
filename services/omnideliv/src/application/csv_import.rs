//! CSV → `IngestedItem`, for the vendor class that has no e-commerce at all.
//!
//! Shopify and WooCommerce adapters exist for merchants who already run a shop.
//! Most OmniDeliv vendors do not: a carinderia, a neighbourhood pharmacy, a
//! sari-sari store. What they have is a spreadsheet. This is their adapter, and
//! it is the only one that needs no credentials, no OAuth and no second system.
//!
//! ## Why this parses server-side
//!
//! The console could parse in the browser and post JSON. Then the parsing rules
//! would live in TypeScript, outside the Rust test suite, and every future
//! client would reimplement them. The console uploads bytes; the rules live
//! here, once, next to the port they feed.
//!
//! ## Why it still cannot declare allergens
//!
//! A vendor typed this file, so it is tempting to treat its allergen column as
//! a declaration. It is not, for the same reason a Shopify tag is not: the file
//! may equally have been exported from a supplier system, and "typed at some
//! point" is not "someone checked this dish". `CatalogSource::Csv` is not
//! `is_human()`, so `merge_ingested` populates `allergens` and leaves
//! `allergens_declared_at` NULL — the vendor confirms per item in the console,
//! which is one tap and is the act that carries the liability.
//!
//! Uploading is likewise not confirming stock. Rows land unconfirmed.

use crate::domain::entities::IngestedItem;

/// A row that could not be imported, and why — carrying its line number.
///
/// Counts alone ("12 rejected") are useless to someone holding a 200-row
/// spreadsheet. The line number is the difference between a report and a
/// scavenger hunt.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RowError {
    /// 1-based, counting the header as line 1 — what the vendor's spreadsheet
    /// shows in its own gutter.
    pub line:   usize,
    pub reason: String,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct CsvImport {
    #[serde(skip)]
    pub items:  Vec<IngestedItem>,
    pub errors: Vec<RowError>,
}

/// Columns we understand. Everything else in the file is ignored rather than
/// rejected — a vendor's spreadsheet has their own columns in it and demanding
/// an exact schema would fail every real file.
const COL_SKU:         &str = "sku";
const COL_NAME:        &str = "name";
const COL_PRICE:       &str = "price";
const COL_DESCRIPTION: &str = "description";
const COL_ALLERGENS:   &str = "allergens";
const COL_DIETARY:     &str = "dietary_tags";
const COL_LISTED:      &str = "listed";

/// Within a cell, `;` separates list values — `,` is the field delimiter and
/// `|` looks like a typo to a spreadsheet user.
const LIST_SEPARATOR: char = ';';

pub fn parse(input: &str) -> Result<CsvImport, String> {
    let rows = split_records(input);
    let Some(header) = rows.first() else {
        return Err("the file is empty".into());
    };

    let idx = |want: &str| header.iter().position(|h| h.trim().eq_ignore_ascii_case(want));

    let (Some(i_sku), Some(i_name), Some(i_price)) =
        (idx(COL_SKU), idx(COL_NAME), idx(COL_PRICE))
    else {
        // Named, not positional. A vendor's column order is theirs, and
        // "column 3 must be price" is a rule nobody reads before saving.
        return Err(format!(
            "the header row needs at least {COL_SKU}, {COL_NAME} and {COL_PRICE} columns \
             (found: {})",
            header.join(", ")
        ));
    };
    let i_desc    = idx(COL_DESCRIPTION);
    let i_allerg  = idx(COL_ALLERGENS);
    let i_dietary = idx(COL_DIETARY);
    let i_listed  = idx(COL_LISTED);

    let mut out = CsvImport::default();

    for (n, row) in rows.iter().enumerate().skip(1) {
        let line = n + 1; // 1-based, header is line 1
        let cell = |i: Option<usize>| i.and_then(|i| row.get(i)).map(|s| s.trim()).unwrap_or("");

        // A wholly blank line is spreadsheet residue, not an error.
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }

        let sku  = cell(Some(i_sku));
        let name = cell(Some(i_name));
        let raw_price = cell(Some(i_price));

        if sku.is_empty() {
            out.errors.push(RowError { line, reason: "no SKU".into() });
            continue;
        }
        if name.is_empty() {
            out.errors.push(RowError { line, reason: "no name".into() });
            continue;
        }
        let Some(price_cents) = parse_price_cents(raw_price) else {
            out.errors.push(RowError {
                line,
                reason: format!("could not read the price {raw_price:?}"),
            });
            continue;
        };

        let desc = cell(i_desc);
        out.items.push(IngestedItem {
            external_id: None, // CSV has no stable foreign id; SKU is the key
            sku:         sku.to_owned(),
            name:        name.to_owned(),
            description: (!desc.is_empty()).then(|| desc.to_owned()),
            price_cents,
            allergens:    split_list(cell(i_allerg)),
            dietary_tags: split_list(cell(i_dietary)),
            // Absent column means listed. A vendor who never heard of the
            // column did not mean "hide everything".
            is_listed:    match i_listed {
                Some(_) => parse_bool(cell(i_listed)),
                None    => true,
            },
        });
    }

    Ok(out)
}

/// `"180.00"`, `"₱1,180.50"`, `"1 180"` → cents.
///
/// Real spreadsheets carry currency symbols and thousands separators because
/// the cell was formatted as currency. Refusing those would reject most files
/// a vendor actually has, so strip anything that is not a digit, separator or
/// sign, then decide which separator was decimal.
fn parse_price_cents(raw: &str) -> Option<i64> {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',' || *c == '-')
        .collect();
    if cleaned.is_empty() || cleaned.starts_with('-') {
        return None;
    }

    // Whichever separator appears last is the decimal one — "1,180.50" and
    // "1.180,50" are both real conventions and both unambiguous by position.
    let normalised = match (cleaned.rfind('.'), cleaned.rfind(',')) {
        (Some(dot), Some(comma)) => {
            if dot > comma {
                cleaned.replace(',', "")                              // 1,180.50
            } else {
                cleaned.replace('.', "").replace(',', ".")            // 1.180,50
            }
        }
        (Some(_), None) => cleaned.clone(),
        (None, Some(_)) => {
            // A lone comma is a decimal comma only when it looks like one;
            // "1,180" is far more likely thousands than 1.18.
            let parts: Vec<&str> = cleaned.split(',').collect();
            if parts.len() == 2 && parts[1].len() <= 2 {
                cleaned.replace(',', ".")
            } else {
                cleaned.replace(',', "")
            }
        }
        (None, None) => cleaned.clone(),
    };

    normalised.parse::<f64>().ok().map(|f| (f * 100.0).round() as i64)
}

fn split_list(cell: &str) -> Vec<String> {
    cell.split(LIST_SEPARATOR)
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_bool(cell: &str) -> bool {
    !matches!(
        cell.trim().to_lowercase().as_str(),
        "0" | "false" | "no" | "n" | "hidden" | "unlisted"
    )
}

/// Split CSV text into records of fields.
fn split_records(input: &str) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut field   = String::new();
    let mut record  = Vec::new();
    let mut chars   = input.strip_prefix('\u{feff}').unwrap_or(input).chars().peekable();
    let mut quoted  = false;

    while let Some(c) = chars.next() {
        match c {
            '"' if quoted => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            }
            '"' => quoted = true,
            ',' if !quoted => record.push(std::mem::take(&mut field)),
            '\r' if !quoted => {}
            '\n' if !quoted => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            other => field.push(other),
        }
    }

    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_are_matched_by_name_in_any_order() {
        let csv = "price,name,sku\n180.00,Chicken Adobo,ADOBO-1\n";
        let r = parse(csv).expect("parses");
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].sku, "ADOBO-1");
        assert_eq!(r.items[0].name, "Chicken Adobo");
        assert_eq!(r.items[0].price_cents, 18000);
    }

    #[test]
    fn a_missing_required_column_fails_the_whole_file_with_a_readable_reason() {
        let err = parse("name,price\nAdobo,180\n").unwrap_err();
        assert!(err.contains("sku"), "must name what is missing: {err}");
        assert!(err.contains("name, price"), "and show what it found: {err}");
    }

    /// The point of the line numbers. One bad row must not discard the file,
    /// and the vendor has to be told which row to fix.
    #[test]
    fn a_bad_row_is_reported_by_line_and_the_rest_still_import() {
        let csv = "sku,name,price\n\
                   A-1,Adobo,180.00\n\
                   A-2,Pancit,not-a-price\n\
                   A-3,Lumpia,95\n";
        let r = parse(csv).expect("parses");

        assert_eq!(r.items.len(), 2, "the good rows still import");
        assert_eq!(r.errors.len(), 1);
        assert_eq!(r.errors[0].line, 3, "1-based, header counted — matches the spreadsheet gutter");
        assert!(r.errors[0].reason.contains("price"));
    }

    #[test]
    fn a_row_missing_its_sku_or_name_is_reported_not_dropped() {
        let csv = "sku,name,price\n,Adobo,180\nA-2,,180\n";
        let r = parse(csv).expect("parses");
        assert!(r.items.is_empty());
        assert_eq!(r.errors.len(), 2);
        assert_eq!(r.errors[0].line, 2);
        assert_eq!(r.errors[1].line, 3);
    }

    /// A description with a comma in it is the single most common thing in a
    /// real menu export, and the thing a naive split on ',' destroys.
    #[test]
    fn a_quoted_field_may_contain_the_delimiter() {
        let csv = "sku,name,price,description\n\
                   A-1,Adobo,180,\"with rice, egg and atchara\"\n";
        let r = parse(csv).expect("parses");
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].description.as_deref(), Some("with rice, egg and atchara"));
    }

    /// Excel writes a doubled quote for a literal one, and product names
    /// contain inches and nicknames constantly.
    #[test]
    fn a_doubled_quote_is_a_literal_quote() {
        let csv = "sku,name,price\nA-1,\"Kuya's \"\"Special\"\" Adobo\",180\n";
        let r = parse(csv).expect("parses");
        assert_eq!(r.items[0].name, "Kuya's \"Special\" Adobo");
    }

    /// Multi-line descriptions survive a spreadsheet export as quoted fields
    /// containing newlines. Splitting on '\n' first would corrupt every row
    /// after one.
    #[test]
    fn a_newline_inside_a_quoted_field_does_not_end_the_record() {
        let csv = "sku,name,price,description\n\
                   A-1,Adobo,180,\"line one\nline two\"\n\
                   A-2,Pancit,150,plain\n";
        let r = parse(csv).expect("parses");
        assert_eq!(r.items.len(), 2, "the embedded newline must not split the record");
        assert_eq!(r.items[0].description.as_deref(), Some("line one\nline two"));
        assert_eq!(r.items[1].sku, "A-2");
    }

    /// Excel on Windows writes CRLF and a UTF-8 BOM. Both are invisible to the
    /// vendor and both break a naive parser — the BOM by corrupting the first
    /// header name so `sku` is never found.
    #[test]
    fn a_windows_export_with_bom_and_crlf_parses() {
        let csv = "\u{feff}sku,name,price\r\nA-1,Adobo,180.00\r\n";
        let r = parse(csv).expect("parses");
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].sku, "A-1");
    }

    #[test]
    fn prices_survive_currency_symbols_and_thousands_separators() {
        for (raw, expected) in [
            ("180",        18000),
            ("180.00",     18000),
            ("180.5",      18050),
            ("₱180.00",    18000),
            ("PHP 180.00", 18000),
            ("1,180.50",   118_050),
            ("1.180,50",   118_050),
            ("1,180",      118_000),
            ("1,18",       118),
        ] {
            let csv = format!("sku,name,price\nA-1,X,\"{raw}\"\n");
            let r = parse(&csv).expect("parses");
            assert_eq!(r.items.first().map(|i| i.price_cents), Some(expected), "price {raw:?}");
        }
    }

    #[test]
    fn a_negative_or_unreadable_price_is_an_error_not_a_zero() {
        for raw in ["-5", "free", ""] {
            let csv = format!("sku,name,price\nA-1,X,{raw}\n");
            let r = parse(&csv).expect("parses");
            assert!(r.items.is_empty(), "price {raw:?} must not import");
            assert_eq!(r.errors.len(), 1, "price {raw:?} must be reported");
        }
    }

    #[test]
    fn list_cells_split_on_semicolons_and_normalise() {
        let csv = "sku,name,price,allergens,dietary_tags\n\
                   A-1,Adobo,180,\"Peanuts; Dairy\",Halal\n";
        let r = parse(csv).expect("parses");
        assert_eq!(r.items[0].allergens, vec!["peanuts".to_string(), "dairy".to_string()]);
        assert_eq!(r.items[0].dietary_tags, vec!["halal".to_string()]);
    }

    #[test]
    fn an_absent_listed_column_means_listed() {
        let r = parse("sku,name,price\nA-1,X,180\n").expect("parses");
        assert!(r.items[0].is_listed);
    }

    #[test]
    fn the_listed_column_understands_how_people_write_no() {
        let csv = "sku,name,price,listed\n\
                   A-1,X,180,no\nA-2,Y,180,FALSE\nA-3,Z,180,0\nA-4,W,180,yes\n";
        let r = parse(csv).expect("parses");
        let listed: Vec<bool> = r.items.iter().map(|i| i.is_listed).collect();
        assert_eq!(listed, vec![false, false, false, true]);
    }

    #[test]
    fn blank_lines_are_residue_not_errors() {
        let csv = "sku,name,price\nA-1,X,180\n\n\nA-2,Y,190\n";
        let r = parse(csv).expect("parses");
        assert_eq!(r.items.len(), 2);
        assert!(r.errors.is_empty(), "trailing blank lines must not be reported as failures");
    }

    #[test]
    fn an_empty_file_is_refused_rather_than_reported_as_a_sync_of_nothing() {
        assert!(parse("").is_err());
    }

    /// Columns the vendor keeps for themselves must not fail the import.
    #[test]
    fn unknown_columns_are_ignored() {
        let csv = "sku,name,price,supplier,cost\nA-1,Adobo,180,Acme,90\n";
        let r = parse(csv).expect("parses");
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].price_cents, 18000, "the *selling* price column, not cost");
    }
}

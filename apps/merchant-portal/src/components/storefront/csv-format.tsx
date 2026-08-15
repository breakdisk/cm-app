"use client";
/**
 * What a catalog CSV has to look like.
 *
 * The importer has understood this format since it shipped, and the console
 * never said so: there was an "Import CSV" button, a file picker, and no
 * indication anywhere of what the file needed. A vendor found out by picking
 * their spreadsheet and reading a rejection — "the header row needs at least
 * sku, name and price columns" — which is the wrong moment to learn a format.
 *
 * Kept next to the button rather than in a doc, because the question is only
 * ever asked with a file already in hand.
 *
 * These columns mirror `services/omnideliv/src/application/csv_import.rs`.
 * scripts/check-csv-columns.sh fails the build if the two drift apart.
 */
import { useState } from "react";
import { ChevronDown, ChevronRight, Download } from "lucide-react";

/** Required — the importer rejects a file whose header lacks any of these. */
const REQUIRED: { col: string; note: string }[] = [
  { col: "sku", note: "Your code for the item. Also the key a re-import updates on." },
  { col: "name", note: "What the customer sees." },
  { col: "price", note: "₱ and thousands separators are fine — “₱1,180.50” reads correctly." },
];

/** Optional — absent is a meaning, not a blank. */
const OPTIONAL: { col: string; note: string }[] = [
  { col: "description", note: "Shown under the name." },
  { col: "allergens", note: "Separate with ; — “soy;dairy”. Absent ≠ none; see below." },
  { col: "dietary_tags", note: "Separate with ; — “halal;vegetarian”." },
  { col: "listed", note: "true/false. Leave the column out and everything is listed." },
];

const TEMPLATE = [
  "sku,name,price,description,allergens,dietary_tags,listed",
  "ADOBO-1,Chicken Adobo,180.00,with garlic rice,soy;dairy,,true",
  "HALO-2,Halo-Halo,95.00,,dairy,vegetarian,true",
  "SILOG-3,Tapsilog,170.00,beef tapa with egg,,,true",
].join("\n");

function downloadTemplate() {
  // Built in the browser rather than served: it is four lines, and a static
  // asset is one more thing that can drift from the parser.
  const blob = new Blob([TEMPLATE], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = "storefront-items-template.csv";
  a.click();
  URL.revokeObjectURL(url);
}

export function CsvFormat() {
  const [open, setOpen] = useState(false);

  return (
    <div className="w-full">
      <div className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          onClick={() => setOpen((o) => !o)}
          aria-expanded={open}
          className="flex items-center gap-1 text-xs text-white/50 hover:text-white/80"
        >
          {open ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
          What does the CSV need?
        </button>
        <button
          type="button"
          onClick={downloadTemplate}
          className="flex items-center gap-1 text-xs text-cyan-300/80 hover:text-cyan-200"
        >
          <Download className="h-3.5 w-3.5" /> Download template
        </button>
      </div>

      {open && (
        <div className="mt-2 space-y-3 rounded-lg border border-white/10 bg-white/[0.03] p-3">
          <div>
            <p className="mb-1 text-[11px] uppercase tracking-wider text-white/40">Required</p>
            <dl className="space-y-1">
              {REQUIRED.map((c) => (
                <div key={c.col} className="flex flex-col gap-0.5 sm:flex-row sm:gap-2">
                  <dt className="shrink-0 font-mono text-xs text-cyan-300">{c.col}</dt>
                  <dd className="text-[11px] text-white/50">{c.note}</dd>
                </div>
              ))}
            </dl>
          </div>

          <div>
            <p className="mb-1 text-[11px] uppercase tracking-wider text-white/40">Optional</p>
            <dl className="space-y-1">
              {OPTIONAL.map((c) => (
                <div key={c.col} className="flex flex-col gap-0.5 sm:flex-row sm:gap-2">
                  <dt className="shrink-0 font-mono text-xs text-white/60">{c.col}</dt>
                  <dd className="text-[11px] text-white/50">{c.note}</dd>
                </div>
              ))}
            </dl>
          </div>

          <div className="space-y-1 border-t border-white/5 pt-2 text-[11px] text-white/45">
            <p>
              Columns are matched <strong className="text-white/70">by name, in any order</strong>,
              and any extra columns in your file are ignored — you can upload the sheet you already
              keep.
            </p>
            <p>
              A row that cannot be read is reported with its line number and the rest of the file
              still imports.
            </p>
            {/* The one that costs a customer something if it is misunderstood. */}
            <p className="text-amber-300/80">
              Imported items arrive <strong>unconfirmed</strong>, and an allergen column you did not
              fill in means “not stated”, not “none”. Until a person confirms an item, the agent
              treats it as substitutable and withholds it from anyone who named an allergy.
            </p>
          </div>

          <pre className="overflow-x-auto rounded border border-white/10 bg-black/30 p-2 font-mono text-[10px] leading-relaxed text-white/60">
            {TEMPLATE}
          </pre>
        </div>
      )}
    </div>
  );
}

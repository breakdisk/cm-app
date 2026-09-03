"use client";
/**
 * The QR codes, on screen and on paper.
 *
 * The original requirement was that the system generates the code for printing,
 * and until now it produced only a URL string — there was no QR anywhere in the
 * platform. This renders it.
 *
 * **The code is generated in the browser, from the `scan_url` the server
 * returned.** Never via an image service: the token IS the credential printed
 * on the table, and handing it to a third-party QR renderer would post every
 * table's secret to someone else's access log.
 *
 * Printing is a `visibility` swap rather than a separate route, so what an
 * operator sees on screen is exactly what comes out of the printer — the two
 * cannot drift.
 */
import { useState } from "react";
import QRCodeSVG from "react-qr-code";
import { Check, Copy, Loader2, Printer, RotateCw } from "lucide-react";

import { venuesApi, type TableRow } from "@/lib/api/venues";

/**
 * Only the sheet reaches the paper. `visibility` rather than `display` so the
 * grid keeps its geometry — collapsing the layout at print time is how these
 * end up one-per-page.
 */
const PRINT_CSS = `
@media print {
  body * { visibility: hidden !important; }
  #table-print-sheet, #table-print-sheet * { visibility: visible !important; }
  #table-print-sheet {
    position: absolute !important;
    left: 0; top: 0; width: 100%;
    padding: 0 !important;
  }
  .print-card {
    break-inside: avoid;
    page-break-inside: avoid;
    border: 1px solid #d4d4d4 !important;
    background: #fff !important;
  }
  .no-print { display: none !important; }
}
`;

export function TablePrintSheet({
  venueName,
  tables,
  onChanged,
}: {
  venueName: string;
  tables: TableRow[];
  onChanged: () => void;
}) {
  const [busy, setBusy] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const rotate = async (t: TableRow) => {
    // Rotation kills the sticker currently on that table. Cheap to do and
    // impossible to undo, so it asks first.
    if (
      !window.confirm(
        `Replace the code for ${t.label}?\n\nThe code currently printed on that table stops working immediately, and you will need to print and stick the new one.`,
      )
    ) {
      return;
    }
    setBusy(t.table_id);
    setError(null);
    try {
      await venuesApi.rotate(t.table_id);
      onChanged();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not rotate that code");
    } finally {
      setBusy(null);
    }
  };

  const copy = async (t: TableRow) => {
    try {
      await navigator.clipboard.writeText(t.scan_url);
      setCopied(t.table_id);
      setTimeout(() => setCopied(null), 1500);
    } catch {
      setError("Could not copy — your browser blocked clipboard access.");
    }
  };

  /**
   * Printing is the only way `printed_at` gets stamped. It is stamped for every
   * table on the sheet, because that is what the operator just sent to paper —
   * the alternative, a per-table tick afterwards, is a step nobody does, and an
   * unstamped table is indistinguishable from one whose code was rotated and
   * never reprinted.
   */
  const printAll = async () => {
    window.print();
    setError(null);
    try {
      await Promise.all(tables.map((t) => venuesApi.markPrinted(t.table_id)));
      onChanged();
    } catch {
      // The paper came out either way. Losing the stamp is a stale badge, not a
      // broken code, so it must not read as a failed print.
      setError("Printed, but could not record it. The codes themselves are fine.");
    }
  };

  if (tables.length === 0) {
    return (
      <p className="rounded-lg border border-white/10 bg-white/[0.02] px-4 py-8 text-center text-sm text-white/40">
        No tables yet. Add some above and their codes will appear here, ready to print.
      </p>
    );
  }

  const unprinted = tables.filter((t) => t.printed_at === null).length;

  return (
    <div className="space-y-4">
      <style>{PRINT_CSS}</style>

      <div className="no-print flex flex-wrap items-center justify-between gap-3">
        <div>
          <h3 className="font-heading text-base font-semibold text-white">
            Printable codes
          </h3>
          <p className="text-sm text-white/50">
            {tables.length} {tables.length === 1 ? "table" : "tables"}
            {unprinted > 0 && (
              <span className="text-amber-signal"> · {unprinted} not yet printed</span>
            )}
          </p>
        </div>
        <button
          onClick={printAll}
          className="inline-flex items-center gap-2 rounded-lg border border-cyan-neon/40 bg-cyan-neon/10 px-4 py-2 text-sm font-medium text-cyan-neon transition hover:bg-cyan-neon/20"
        >
          <Printer className="h-4 w-4" />
          Print all
        </button>
      </div>

      {error && (
        <p className="no-print rounded-lg border border-amber-signal/30 bg-amber-signal/10 px-3 py-2 text-sm text-amber-signal">
          {error}
        </p>
      )}

      <div
        id="table-print-sheet"
        className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3"
      >
        {tables.map((t) => (
          <div
            key={t.table_id}
            className="print-card flex flex-col items-center gap-3 rounded-xl border border-white/10 bg-white/[0.03] p-4 text-center backdrop-blur-xl"
          >
            {/* White tile on purpose: a QR needs a light quiet zone and high
                contrast, and this is also exactly how it prints. */}
            <div className="rounded-lg bg-white p-3">
              <QRCodeSVG
                value={t.scan_url}
                size={148}
                level="M"
                style={{ height: "auto", maxWidth: "100%", width: "148px" }}
              />
            </div>

            <div>
              <p className="font-heading text-xl font-bold text-white print:text-black">
                {t.label}
              </p>
              <p className="text-xs text-white/50 print:text-neutral-600">{venueName}</p>
              <p className="mt-1 text-xs text-white/40 print:text-neutral-500">
                Scan to see the menu and order
              </p>
            </div>

            <div className="no-print flex w-full flex-wrap items-center justify-center gap-2 border-t border-white/5 pt-3">
              <span
                className={
                  t.status === "open"
                    ? "rounded-full bg-green-signal/15 px-2 py-0.5 text-[11px] font-medium text-green-signal"
                    : "rounded-full bg-white/10 px-2 py-0.5 text-[11px] font-medium text-white/50"
                }
              >
                {t.status}
              </span>
              <span
                className={
                  t.printed_at
                    ? "text-[11px] text-white/40"
                    : "text-[11px] text-amber-signal"
                }
              >
                {t.printed_at ? "printed" : "not printed"}
              </span>

              <button
                onClick={() => copy(t)}
                title="Copy the scan link"
                aria-label={`Copy the scan link for ${t.label}`}
                className="ml-auto rounded-md border border-white/10 bg-white/5 p-1.5 text-white/60 transition hover:bg-white/10 hover:text-white"
              >
                {copied === t.table_id ? (
                  <Check className="h-3.5 w-3.5 text-green-signal" />
                ) : (
                  <Copy className="h-3.5 w-3.5" />
                )}
              </button>
              <button
                onClick={() => rotate(t)}
                disabled={busy === t.table_id}
                title="Replace this code — the printed one stops working"
                aria-label={`Replace the code for ${t.label}`}
                className="rounded-md border border-white/10 bg-white/5 p-1.5 text-white/60 transition hover:bg-white/10 hover:text-amber-signal disabled:opacity-40"
              >
                {busy === t.table_id ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <RotateCw className="h-3.5 w-3.5" />
                )}
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

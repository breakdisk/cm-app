"use client";
import { useEffect, useRef, useState } from "react";
import { motion } from "framer-motion";
import { Loader2, Upload, X } from "lucide-react";
import { fetchDocumentTypes, uploadDocumentOnBehalf } from "@/lib/api/compliance";
import type { DocumentType } from "@/lib/api/compliance";
import {
  ACCEPTED_CONTENT_TYPES,
  MAX_DOCUMENT_NUMBER_LENGTH,
  nextFocusIndex,
  readFileAsBase64,
  validateUploadFile,
} from "@/lib/compliance/upload";

interface Props {
  profileId: string;
  onClose:   () => void;
  /** Called after a successful upload, before the modal closes. */
  onUploaded: () => void;
}

const INPUT =
  "w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-sm text-white placeholder:text-white/25 outline-none focus:border-cyan-glow/50";
const FOCUSABLE =
  'a[href], button, input, select, textarea, [tabindex]:not([tabindex="-1"])';
const LABEL = "text-[11px] uppercase tracking-wider text-white/40";

/**
 * Upload a document into a courier's profile on their behalf (compliance:admin).
 * The parent decides whether to render this at all; the server enforces the
 * permission regardless.
 */
export function UploadDocumentModal({ profileId, onClose, onUploaded }: Props) {
  const [types,     setTypes]     = useState<DocumentType[]>([]);
  const [typesErr,  setTypesErr]  = useState<string | null>(null);
  const [typeId,    setTypeId]    = useState("");
  const [number,    setNumber]    = useState("");
  const [issue,     setIssue]     = useState("");
  const [expiry,    setExpiry]    = useState("");
  const [file,      setFile]      = useState<File | null>(null);
  const [fileErr,   setFileErr]   = useState<string | null>(null);
  const [busy,      setBusy]      = useState(false);
  const [error,     setError]     = useState<string | null>(null);

  const dialogRef = useRef<HTMLDivElement>(null);
  const firstFieldRef = useRef<HTMLSelectElement>(null);
  // Latest busy/onClose for the document-level key handler without re-binding it.
  const busyRef = useRef(busy);
  busyRef.current = busy;
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  // Focus management: remember the trigger, move focus in, restore on close.
  useEffect(() => {
    const trigger = document.activeElement as HTMLElement | null;
    firstFieldRef.current?.focus();

    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        if (busyRef.current) return;
        e.preventDefault();
        onCloseRef.current();
        return;
      }
      if (e.key !== "Tab") return;
      const root = dialogRef.current;
      if (!root) return;
      const items = Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
        (el) => !(el as HTMLButtonElement).disabled,
      );
      const next = nextFocusIndex(items.indexOf(document.activeElement as HTMLElement), items.length, e.shiftKey);
      e.preventDefault();
      if (next !== null) items[next].focus();
    }
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      trigger?.focus();
    };
  }, []);

  useEffect(() => {
    let live = true;
    fetchDocumentTypes()
      .then((t) => { if (live) setTypes(t); })
      .catch((e) => {
        if (live) setTypesErr(e instanceof Error ? e.message : "Could not load document types.");
      });
    return () => { live = false; };
  }, []);

  function onFile(f: File | null) {
    setError(null);
    if (!f) { setFile(null); setFileErr(null); return; }
    const problem = validateUploadFile(f);
    setFileErr(problem);
    setFile(problem ? null : f);
  }

  const trimmedNumber = number.trim();
  const canSubmit =
    !busy && !!typeId && trimmedNumber.length > 0 &&
    trimmedNumber.length <= MAX_DOCUMENT_NUMBER_LENGTH && !!file;

  async function submit() {
    if (!canSubmit || !file) return;
    setBusy(true);
    setError(null);
    try {
      const file_base64 = await readFileAsBase64(file);
      await uploadDocumentOnBehalf(profileId, {
        document_type_id: typeId,
        document_number:  trimmedNumber,
        file_base64,
        content_type:     file.type,
        ...(issue  ? { issue_date:  issue }  : {}),
        ...(expiry ? { expiry_date: expiry } : {}),
      });
      onUploaded();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Upload failed.");
      setBusy(false);
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      onClick={busy ? undefined : onClose}
    >
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" />
      <motion.div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="upl-title"
        initial={{ opacity: 0, scale: 0.95, y: 12 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        transition={{ type: "spring", damping: 26, stiffness: 300 }}
        className="relative z-10 w-full max-w-md max-h-[90vh] flex flex-col rounded-2xl border border-white/10 bg-[#0d1422] shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-white/10 px-5 py-3.5">
          <div className="flex items-center gap-2">
            <Upload size={15} className="text-cyan-400" />
            <h2 id="upl-title" className="text-base font-bold text-white">Upload document</h2>
          </div>
          <button
            onClick={onClose}
            disabled={busy}
            aria-label="Close"
            className="rounded-lg p-1.5 text-white/40 hover:bg-white/10 hover:text-white disabled:opacity-40 transition-colors"
          >
            <X size={14} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-5 flex flex-col gap-4">
          <div className="flex flex-col gap-1">
            <label htmlFor="upl-type" className={LABEL}>Document type</label>
            <select
              id="upl-type"
              ref={firstFieldRef}
              value={typeId}
              onChange={(e) => setTypeId(e.target.value)}
              disabled={busy}
              className={INPUT}
            >
              <option value="">Select a type…</option>
              {types.map((t) => (
                <option key={t.id} value={t.id}>{t.name} ({t.jurisdiction})</option>
              ))}
            </select>
            {typesErr && <span className="text-xs text-red-signal/80">{typesErr}</span>}
          </div>

          <div className="flex flex-col gap-1">
            <label htmlFor="upl-number" className={LABEL}>Document number</label>
            <input
              id="upl-number"
              value={number}
              onChange={(e) => setNumber(e.target.value)}
              maxLength={MAX_DOCUMENT_NUMBER_LENGTH}
              disabled={busy}
              placeholder="e.g. N01-23-456789"
              className={`${INPUT} font-mono`}
            />
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div className="flex flex-col gap-1">
              <label htmlFor="upl-issue" className={LABEL}>Issue date (optional)</label>
              <input
                id="upl-issue" type="date" value={issue}
                onChange={(e) => setIssue(e.target.value)}
                disabled={busy} className={INPUT}
              />
            </div>
            <div className="flex flex-col gap-1">
              <label htmlFor="upl-expiry" className={LABEL}>Expiry date (optional)</label>
              <input
                id="upl-expiry" type="date" value={expiry}
                onChange={(e) => setExpiry(e.target.value)}
                disabled={busy} className={INPUT}
              />
            </div>
          </div>

          <div className="flex flex-col gap-1">
            <label htmlFor="upl-file" className={LABEL}>File (JPEG, PNG, WebP or PDF, max 10 MB)</label>
            <input
              id="upl-file"
              type="file"
              accept={ACCEPTED_CONTENT_TYPES.join(",")}
              onChange={(e) => onFile(e.target.files?.[0] ?? null)}
              disabled={busy}
              className="text-xs text-white/60 file:mr-3 file:rounded-lg file:border file:border-glass-border file:bg-glass-100 file:px-3 file:py-1.5 file:text-xs file:text-white/70"
            />
            {fileErr && <span role="alert" className="text-xs text-red-signal/80">{fileErr}</span>}
          </div>

          {error && (
            <div role="alert" className="text-xs font-mono text-red-signal/80 bg-red-surface/10 border border-red-glow/20 rounded-lg px-2.5 py-1.5">
              {error}
            </div>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-white/10 px-5 py-3.5">
          <button
            onClick={onClose}
            disabled={busy}
            className="px-3 py-1.5 rounded-lg text-xs border border-glass-border text-white/50 hover:text-white/80 disabled:opacity-40 transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={submit}
            disabled={!canSubmit}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-bold border border-cyan-glow/35 bg-cyan-surface/20 text-cyan-neon hover:bg-cyan-surface/30 disabled:opacity-40 transition-colors"
          >
            {busy ? <Loader2 className="h-3 w-3 animate-spin" /> : <Upload className="h-3 w-3" />}
            {busy ? "Uploading…" : "Upload"}
          </button>
        </div>
      </motion.div>
    </div>
  );
}

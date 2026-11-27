import { useEffect, useRef, type RefObject } from "react";

const FOCUSABLE =
  'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * Modal-dialog keyboard behaviour (WCAG 2.1 AA): while `open`, focus moves into
 * the dialog, Tab/Shift+Tab cycle inside it, Escape closes it, and on close
 * focus returns to whatever held it before the dialog opened.
 *
 * Attach the returned ref to the dialog container. Give the container
 * `role="dialog"`, `aria-modal="true"`, `aria-labelledby` and `tabIndex={-1}`.
 */
export function useDialogA11y<T extends HTMLElement>(
  open: boolean,
  onClose: () => void,
): RefObject<T> {
  const ref = useRef<T>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    if (!open) return;
    const previouslyFocused = document.activeElement as HTMLElement | null;
    const node = ref.current;

    const focusables = (): HTMLElement[] =>
      node ? Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE)) : [];

    // Move focus in: first focusable, else the container itself.
    const first = focusables()[0];
    if (first) first.focus();
    else node?.focus();

    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.stopPropagation();
        onCloseRef.current();
        return;
      }
      if (e.key !== "Tab" || !node) return;
      const items = focusables();
      if (items.length === 0) {
        e.preventDefault();
        node.focus();
        return;
      }
      const head = items[0];
      const tail = items[items.length - 1];
      const active = document.activeElement;
      if (e.shiftKey && (active === head || !node.contains(active))) {
        e.preventDefault();
        tail.focus();
      } else if (!e.shiftKey && (active === tail || !node.contains(active))) {
        e.preventDefault();
        head.focus();
      }
    }

    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      previouslyFocused?.focus?.();
    };
  }, [open]);

  return ref;
}

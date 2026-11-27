/**
 * `"1-12, patio, 14"` -> twelve numbered labels, `patio`, and `14`.
 *
 * Typing twenty labels by hand is the kind of chore that ends in a venue with
 * six tables set up and the rest done "later".
 */
export function parseLabels(input: string): string[] {
  const out: string[] = [];
  for (const rawPart of input.split(",")) {
    const part = rawPart.trim();
    if (part === "") continue;
    const range = part.match(/^(\d+)\s*-\s*(\d+)$/);
    if (range) {
      const from = Number(range[1]);
      const to = Number(range[2]);
      // Bounded: a typo like 1-100000 must not try to create a hundred thousand
      // tables. The server caps at 200 too; this is the friendlier half.
      if (to >= from && to - from < 200) {
        for (let i = from; i <= to; i++) out.push(String(i));
        continue;
      }
    }
    out.push(part);
  }
  // Dedupe, preserving order — two tables called "12" is a support ticket.
  return Array.from(new Set(out));
}

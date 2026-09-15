/**
 * Reads a move request out of the Home prompt.
 *
 * Client-side and deliberately modest. The part-2 handoff wants intent and
 * parsing on the server ("Classification belongs on the server"); until that
 * exists, this only pre-fills the plan screen. Every value it produces is shown
 * back as an editable field, and nothing is priced from it — the quote is the
 * server's, from what the customer confirms.
 */

/**
 * Exactly the handoff's pattern. Narrow on purpose: "help", "driver" and
 * "broken" are ordinary move vocabulary and must not route to support.
 */
export const SUPPORT_HINTS =
  /damaged|my claim|refund|invoice|receipt|where is my (box|move|delivery|driver|stuff)|reschedul|running late|never arrived|didn't arrive|missing (box|item|carton|piece)|complain|wrong address|overcharg/i;

export type Intent = 'support' | 'book';

/** Support only when the words match and there is a job to ask about. With no
 *  active job the same words are a booking request. */
export function classifyIntent(text: string, hasActiveJob: boolean): Intent {
  return hasActiveJob && SUPPORT_HINTS.test(text) ? 'support' : 'book';
}

export interface ParsedItem {
  name: string;
  qty: number;
}

export interface ParsedMove {
  items: ParsedItem[];
  from: string;
  to: string;
  when: string;
}

// A time needs am/pm or minutes: "at 12 Elm St" is an address, not noon.
const WHEN =
  /\b(today|tonight|tomorrow|this (?:morning|afternoon|evening|weekend)|next week|(?:on )?(?:mon|tues|wednes|thurs|fri|satur|sun)day|at \d{1,2}(?::\d{2}\s*(?:am|pm)?|\s*(?:am|pm)))\b|\b\d{1,2}(?::\d{2})?\s*(?:am|pm)\b/gi;

const LEADING_VERB = /^(?:please\s+)?(?:i\s+(?:want|need)\s+to\s+)?(?:move|moving|haul|deliver|send|ship|take|bring|pick up|get)\s+/i;
const ARTICLE = /^(?:my|the|our|a|an|some)\s+/i;
const NUMBER_WORDS: Record<string, number> = {
  one: 1, two: 2, three: 3, four: 4, five: 5, six: 6, seven: 7, eight: 8, nine: 9, ten: 10,
};

function tidy(s: string): string {
  return s.replace(/\s+/g, ' ').replace(/^[\s,.;:-]+|[\s,.;:-]+$/g, '');
}

function toItem(raw: string): ParsedItem | null {
  const text = tidy(raw.replace(LEADING_VERB, '').replace(ARTICLE, ''));
  if (!text) return null;
  const digits = text.match(/^(\d+)\s+(.+)$/);
  if (digits) return { qty: Number(digits[1]), name: digits[2] };
  // "two pallets", not "one-bedroom apartment": the number word must stand alone.
  const word = text.match(/^([a-z]+)\s+(.+)$/i);
  if (word && NUMBER_WORDS[word[1].toLowerCase()] !== undefined) {
    return { qty: NUMBER_WORDS[word[1].toLowerCase()], name: word[2] };
  }
  return { qty: 1, name: text };
}

export function parsePrompt(input: string): ParsedMove {
  let text = input.trim();

  const when = tidy((text.match(WHEN) ?? []).map(tidy).join(' '));
  text = tidy(text.replace(WHEN, ' '));

  let from = '';
  let to = '';
  const fromTo = text.match(/\bfrom\s+(.+?)\s+to\s+(.+)$/i);
  const toOnly = text.match(/\s+to\s+(.+)$/i);
  if (fromTo && fromTo.index !== undefined) {
    from = tidy(fromTo[1]);
    to = tidy(fromTo[2]);
    text = text.slice(0, fromTo.index);
  } else if (toOnly && toOnly.index !== undefined) {
    to = tidy(toOnly[1]);
    text = text.slice(0, toOnly.index);
  }

  const items = text
    .replace(LEADING_VERB, '')
    .split(/,|\band\b|\+|&/i)
    .map(toItem)
    .filter((i): i is ParsedItem => i !== null);

  return { items, from, to, when };
}

/** "3 items · 1 pickup · 1 drop-off · tomorrow at 2 PM" — the thinking screen's
 *  first line, from what was actually read, never invented. */
export function describeParse(p: ParsedMove): string {
  const count = p.items.reduce((n, i) => n + i.qty, 0);
  const parts = [`${count} item${count === 1 ? '' : 's'}`];
  if (p.from) parts.push('1 pickup');
  if (p.to) parts.push('1 drop-off');
  if (p.when) parts.push(p.when);
  return parts.join(' · ');
}

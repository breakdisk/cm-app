/**
 * The OmniDeliv customer surface has no wallet, and that is a decision.
 *
 * On 2026-08-11 the LogisticOS customer app's Wallet screen was deleted rather
 * than repointed: it was showing the *merchant settlement wallet* — tenant
 * balance, with a working Request Withdrawal button — and the only thing
 * standing between a customer and that money was a permission the customer role
 * happens not to hold.
 *
 * That is guarded on the identity side by a role test. This is the second
 * surface where the same wrong repair is tempting, so it is guarded here too.
 *
 * If this test fails because someone is building a customer wallet: that is a
 * regulated stored-value product (BSP e-money in the PH market) needing a top-up
 * rail, refund-to-balance semantics and its own ADR. Reopen the decision rather
 * than deleting this test.
 */
import fs from "fs";
import path from "path";

const SRC = path.resolve(__dirname, "../..");
const APP = path.resolve(__dirname, "../../../app");

function sourceFiles(dir: string): string[] {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const full = path.join(dir, e.name);
    if (e.isDirectory()) return e.name === "__tests__" ? [] : sourceFiles(full);
    return /\.tsx?$/.test(e.name) ? [full] : [];
  });
}

/**
 * Strip `//` line comments and `/* *\/` block comments. Used only by the
 * "withdraw or top-up control" check below: money.ts and MoneyPanel.tsx
 * deliberately talk about "no top-up" and "no withdraw" in comments to
 * document *why* there is no wallet, and that prose would otherwise trip
 * a naive match. The other two checks below deliberately run on the raw,
 * uncommented text — a "wallet balance" or a `/v1/wallet` path is worth
 * flagging even if someone lands it inside a comment first (a comment is
 * often the first draft of the code that follows), and narrowing them
 * would blind them to exactly that.
 *
 * This is a simple stripper, not a full tokenizer: it does not special-case
 * string/template literals that contain `//` or `/*`, so a comment marker
 * inside a string could throw off the state. None of this codebase's sources
 * do that today; if one starts to, prefer fixing the offending literal over
 * growing this into a real lexer.
 */
function stripComments(text: string): string {
  let out = "";
  let i = 0;
  while (i < text.length) {
    const two = text.slice(i, i + 2);
    if (two === "//") {
      const nl = text.indexOf("\n", i);
      i = nl === -1 ? text.length : nl;
      continue;
    }
    if (two === "/*") {
      const end = text.indexOf("*/", i + 2);
      i = end === -1 ? text.length : end + 2;
      continue;
    }
    out += text[i];
    i += 1;
  }
  return out;
}

const sources = [...sourceFiles(SRC), ...sourceFiles(APP)].map((f) => {
  const text = fs.readFileSync(f, "utf8");
  return { file: f, text, code: stripComments(text) };
});

describe("the customer app has no wallet", () => {
  it("calls no wallet, balance, top-up or withdraw endpoint", () => {
    const offenders = sources.filter((s) =>
      /["'`][^"'`]*\/v1\/(wallet|balance|top-?up|withdraw)/i.test(s.text),
    );
    expect(offenders.map((o) => o.file)).toEqual([]);
  });

  it("renders no withdraw or top-up control", () => {
    const offenders = sources.filter((s) => /withdraw|top[\s-]?up/i.test(s.code));
    expect(offenders.map((o) => o.file)).toEqual([]);
  });

  /** The panel that legitimately shows money must not call itself a balance. */
  it("does not present a balance", () => {
    const offenders = sources.filter((s) => /wallet balance|available balance/i.test(s.text));
    expect(offenders.map((o) => o.file)).toEqual([]);
  });
});

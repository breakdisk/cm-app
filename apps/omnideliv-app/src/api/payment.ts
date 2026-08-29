/**
 * Handing the customer to Network International's hosted card page, and taking
 * them back.
 *
 * Two rules shape everything here.
 *
 * **The card page is not ours.** Card details are entered on NI's own page in a
 * browser, never in this app — that is the whole reason the platform stays in
 * PCI SAQ-A scope. There is no card form in this codebase and there must never
 * be one.
 *
 * **The browser's answer is not the payment's answer.** Whatever
 * `openAuthSessionAsync` resolves with — `success`, `cancel`, `dismiss` — says
 * only what the browser did, not what the money did. A customer can pay and
 * then kill the browser; a customer can be redirected back before NI has told
 * us anything. The authorization is real when `payment.intent.authorized`
 * reaches the server from NI's webhook and not a moment before, so every caller
 * here re-reads the order from the server afterwards rather than believing this
 * return value. That is why this returns `void`: there is deliberately no
 * "paid" boolean to be tempted by.
 *
 * The redirect target is the app's own scheme so the browser closes itself when
 * NI redirects, but nothing depends on that: if NI lands on the https return
 * page instead, the customer taps Done and the caller polls exactly the same
 * way.
 */
import * as Linking from "expo-linking";
import * as WebBrowser from "expo-web-browser";

/** Where NI's redirect should land to auto-close the browser: `omnideliv://payment/return`. */
export function paymentReturnUrl(): string {
  return Linking.createURL("payment/return");
}

/**
 * Opens the hosted card page and resolves once the browser is closed, for any
 * reason. Never throws for an abandoned payment — abandoning is a normal
 * outcome, and the server releases the order on its own timer.
 */
export async function openHostedCheckout(checkoutUrl: string): Promise<void> {
  try {
    await WebBrowser.openAuthSessionAsync(checkoutUrl, paymentReturnUrl());
  } finally {
    // Android keeps the custom-tab session warm otherwise, and a stale session
    // makes the *next* checkout open into the previous order's page.
    WebBrowser.dismissBrowser?.();
  }
}

package net.cargomarket.omnideliv.courier.data

import kotlinx.serialization.json.Json

/**
 * The one JSON configuration this app uses.
 *
 * Defined here rather than inside the Hilt module so the wire-contract tests
 * exercise the *same* settings the app ships. A test that built its own `Json`
 * could pass while the app failed — which is precisely how the auth envelope
 * survived: `ignoreUnknownKeys` silently discarded `data`, and nothing was
 * checking with that flag on.
 */
val CourierJson: Json = Json {
    // The server adds fields over time — `offer_card`, `courier_user_id` — and
    // an app that refused an unknown one would break on every backend deploy
    // rather than ignoring what it does not yet render.
    ignoreUnknownKeys = true

    // Null means "not set", so it is omitted rather than sent as an explicit
    // null the server would have to interpret.
    explicitNulls = false

    // Defaults ARE sent. Without this kotlinx omits them, so `role = "driver"`
    // never left the device and sign-in relied on identity happening to default
    // the same way. That assumption is exactly what put customers in the driver
    // role once already — an implicit agreement between two services that
    // neither states.
    encodeDefaults = true
}

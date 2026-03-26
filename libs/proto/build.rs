//! Tonic build script — compiles all .proto files in libs/proto/proto/
//! into Rust gRPC client/server stubs.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = "proto";

    let proto_files = [
        "identity/identity.proto",
        "order/order.proto",
        "dispatch/dispatch.proto",
        "driver/driver.proto",
        "cdp/cdp.proto",
        "engagement/engagement.proto",
        "payments/payments.proto",
        "analytics/analytics.proto",
        "pod/pod.proto",
        "fleet/fleet.proto",
        "carrier/carrier.proto",
    ];

    let proto_paths: Vec<_> = proto_files
        .iter()
        .map(|f| format!("{}/{}", proto_root, f))
        .collect();

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile(
            &proto_paths,
            &[proto_root],
        )?;

    // Re-run if any proto file changes.
    for f in &proto_paths {
        println!("cargo:rerun-if-changed={}", f);
    }

    Ok(())
}

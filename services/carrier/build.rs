fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use the bundled protoc binary so CI doesn't need a system-installed protoc.
    let protoc = protoc_bin_vendored::protoc_bin_path()
        .expect("protoc-bin-vendored: could not find bundled protoc binary");
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_server(true)
        .build_client(false) // clients generated in libs/proto when needed
        .compile(&["proto/carrier.proto"], &["proto"])?;

    println!("cargo:rerun-if-changed=proto/carrier.proto");
    Ok(())
}

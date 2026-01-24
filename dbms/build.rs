fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true) // ENABLED
        .compile(&["../server/proto/agreda.proto"], &["../server/proto"])?;
    Ok(())
}
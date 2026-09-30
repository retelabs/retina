//! Compiles the pinned OTLP trace protos (vendor/opentelemetry-proto @
//! v1.11.0 — see docs/interfaces/otlp-ingestion.md) into Rust. Only the
//! traces signal is compiled: logs/metrics/profiles are out of MVP scope
//! (dossier section 4).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Vendors a prebuilt protoc binary instead of requiring it on the host/CI
    // (no system package manager assumed — dossier step 6: CI/CD must be
    // reproducible regardless of runner image). protoc-bin-vendored ships a
    // prebuilt binary rather than compiling protobuf from source, so it needs
    // no cmake/C++ toolchain on the build machine.
    unsafe {
        std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);
    }

    let proto_root = "../../vendor/opentelemetry-proto";
    println!("cargo:rerun-if-changed={proto_root}");

    // Client codegen was skipped until now (this crate only ever ran the
    // server side) — turned on for dossier step 7, which needs a real gRPC
    // client to replay converted telemetry against a running kernel rather
    // than only calling `convert_span`/`Receiver` in-process.
    tonic_prost_build::configure().compile_protos(
        &[format!(
            "{proto_root}/opentelemetry/proto/collector/trace/v1/trace_service.proto"
        )],
        &[proto_root.to_string()],
    )?;

    Ok(())
}

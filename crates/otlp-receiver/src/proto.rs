//! Generated code from the pinned OTLP trace protos (build.rs). The module
//! nesting mirrors the protobuf package path exactly — required because
//! prost-build emits cross-package references as `super::` paths (verified
//! empirically against the generated output before writing this).

pub mod opentelemetry {
    pub mod proto {
        pub mod common {
            pub mod v1 {
                include!(concat!(
                    env!("OUT_DIR"),
                    "/opentelemetry.proto.common.v1.rs"
                ));
            }
        }
        pub mod resource {
            pub mod v1 {
                include!(concat!(
                    env!("OUT_DIR"),
                    "/opentelemetry.proto.resource.v1.rs"
                ));
            }
        }
        pub mod trace {
            pub mod v1 {
                include!(concat!(env!("OUT_DIR"), "/opentelemetry.proto.trace.v1.rs"));
            }
        }
        pub mod collector {
            pub mod trace {
                pub mod v1 {
                    include!(concat!(
                        env!("OUT_DIR"),
                        "/opentelemetry.proto.collector.trace.v1.rs"
                    ));
                }
            }
        }
    }
}

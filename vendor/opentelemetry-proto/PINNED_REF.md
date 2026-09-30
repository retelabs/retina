Source: https://github.com/open-telemetry/opentelemetry-proto.git
Requested ref: v1.11.0
Resolved commit: 790608c4d51e6ffc12210b541e8514cbed9e91a4
Pinned on: 2026-08-14T11:19:28Z

Do not edit this directory by hand. To change the pin:
scripts/pin-otlp-proto.sh <new-ref>

The tonic/prost receiver (kernel step 2) must compile the .proto files from
this directory, not from an ad hoc copy or a different version pulled in
through a third-party crate.

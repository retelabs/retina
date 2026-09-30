# 0002. Hosting model: self-hosted on bare VMs, provider deferred

Date: 2026-08-16 (discussed and decided between 2026-08-14 and 2026-08-16;
recorded here)

Status: Accepted in principle. The choice of provider is deliberately **not**
made by this ADR.

## Context

The design dossier (section 5) asked "GCP or Azure for production?". The
discussion with the owner shifted the question on 2026-08-14: the real stake was
not the provider's logo but learning, by coding it live, the concepts and method
to build (and cost) one's own infrastructure pieces rather than consuming
ready-made managed services. The criterion was refined on 2026-08-15: managed
services are not rejected outright; what must be avoided is specifically cost
**billed regardless of usage** (managed Kubernetes control-plane fees even at
zero pods, managed databases billed per provisioned instance, capacity reserved
by the hour).

## Decision

Build on bare hardware (one VM/VPS) plus Docker, with a home-made control plane
(`crates/orchestrator`) rather than provider-specific managed cloud services.
Explicit criterion for any candidate component: compute its **cost at zero
usage** before adopting it (`crates/cost-model`, not guessed); prefer coding it
oneself over consuming it when the learning value justifies it, engineering cost
deciding only when it far exceeds what the work teaches.

The hosting provider itself **is not decided here**. Hetzner serves as a verified
reference point in `crates/cost-model` (official price confirmed, not a choice of
provider); OVH, Scaleway and DigitalOcean were mentioned without being evaluated.

## Consequences

The Docker images (`docker/kernel.Dockerfile`, `docker/query-api.Dockerfile`)
and `crates/orchestrator` are already portable to any VM provider: no code change
is needed when choosing, only provisioning will depend on it (consistent with the
intent set at step 6 of the kernel). The accepted downside: no autoscaling, no
managed high availability, no provider support. Everything a managed cloud would
offer is either built by hand or accepted as missing (consistent with the
existing MVP scope: HA and multi-region are out, design dossier section 4).

## Alternatives considered

- **GKE/AKS** (managed Kubernetes): set aside, control-plane fees are billed even
  at zero pods, against the zero-usage cost criterion.
- **ClickHouse Cloud / managed databases with provisioned capacity**: set aside
  for the same reason (details: ADR-0003).
- **Staying entirely on third-party pay-per-use services**, building nothing:
  set aside, against the learning goal the owner set explicitly, which is the
  real motivation of this ADR rather than cost alone.

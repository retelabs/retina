//! Prices checked against the providers' official pages on 2026-08-16,
//! never aggregators (a first search through aggregator sites gave
//! contradictory figures for Hetzner, set aside for `docs.hetzner.com`
//! itself). Full sources in `docs/cost-model.md`.
//!
//! These prices move (Hetzner had an adjustment in mid-2026 that renamed and
//! repriced the CX22 plan as CX23): re-check them before any real decision,
//! do not rely on having read them once here.

pub struct VmPricing {
    pub monthly_eur: f64,
    pub included_disk_gb: f64,
    pub label: &'static str,
}

/// Hetzner Cloud's entry plan (2 vCPU / 4 GB RAM / 40 GB disk included),
/// excluding IPv4 and VAT. Source: docs.hetzner.com/general/infrastructure-and-availability/price-adjustment/,
/// checked 2026-08-16. The plan was called CX22 before the mid-2026 price
/// adjustment renamed it CX23.
pub const HETZNER_CX23: VmPricing = VmPricing {
    monthly_eur: 5.49,
    included_disk_gb: 40.0,
    label: "Hetzner CX23 (formerly CX22, 2 vCPU / 4 GB RAM)",
};

pub struct ObjectStoragePricing {
    pub per_gb_month_usd: f64,
    pub free_gb: f64,
    pub label: &'static str,
}

/// Source: backblaze.com/cloud-storage/pricing, checked 2026-08-16:
/// $6.95 per TB a month, 10 GB free, free egress up to 3x the stored volume
/// (beyond that $0.01 per GB, not modelled here: this work computes storage
/// cost only, not egress).
pub const BACKBLAZE_B2: ObjectStoragePricing = ObjectStoragePricing {
    per_gb_month_usd: 6.95 / 1000.0,
    free_gb: 10.0,
    label: "Backblaze B2",
};

/// Source: cloudflare.com/plans, checked 2026-08-16: the free plan's CDN is
/// unmetered (no per-request cost), unlike Workers (compute), which are
/// metered. It is the one of the three hybrid additions of the "Venice
/// Deployment" diagram that stays at €0 at *any* realistic volume for this
/// project, not only at zero volume.
pub const CDN_MONTHLY_EUR: f64 = 0.0;

/// GitHub Container Registry (ghcr.io/retelabs, since the 2026-09-30
/// migration). Checked on docs.github.com (billing, GitHub Packages): free
/// for public packages; for private ones the organisation Free plan includes
/// 500 MB of storage and 1 GB of transfer a month, blocked beyond that
/// without a payment method. CI pushes an image only on a `v*` tag or by
/// hand, which stays within that quota, hence €0.
pub const CONTAINER_REGISTRY_MONTHLY_EUR: f64 = 0.0;

//! Prix vérifiés contre les pages officielles des fournisseurs le
//! 2026-08-16 — jamais des agrégateurs (une première recherche via des
//! sites d'agrégation a donné des chiffres contradictoires pour Hetzner,
//! écartés au profit de `docs.hetzner.com` directement). Détail complet
//! des sources dans `docs/cost-model.md`.
//!
//! Ces prix bougent (Hetzner a justement eu un ajustement mi-2026, qui a
//! renommé/repricé le plan CX22 en CX23) — à revérifier avant toute
//! décision réelle, pas seulement lus une fois ici.

pub struct VmPricing {
    pub monthly_eur: f64,
    pub included_disk_gb: f64,
    pub label: &'static str,
}

/// Plan d'entrée Hetzner Cloud (2 vCPU / 4 Go RAM / 40 Go disque inclus),
/// hors IPv4 et hors TVA. Source : docs.hetzner.com/general/infrastructure-and-availability/price-adjustment/,
/// vérifié 2026-08-16 — le plan s'appelait CX22 avant l'ajustement de prix
/// de mi-2026 qui l'a renommé CX23.
pub const HETZNER_CX23: VmPricing = VmPricing {
    monthly_eur: 5.49,
    included_disk_gb: 40.0,
    label: "Hetzner CX23 (ex-CX22, 2 vCPU / 4 Go RAM)",
};

pub struct ObjectStoragePricing {
    pub per_gb_month_usd: f64,
    pub free_gb: f64,
    pub label: &'static str,
}

/// Source : backblaze.com/cloud-storage/pricing, vérifié 2026-08-16 —
/// $6.95/To/mois, 10 Go gratuits, egress gratuit jusqu'à 3x le stockage
/// (au-delà : $0.01/Go, non modélisé ici, ce chantier ne calcule que le
/// coût de stockage, pas l'egress).
pub const BACKBLAZE_B2: ObjectStoragePricing = ObjectStoragePricing {
    per_gb_month_usd: 6.95 / 1000.0,
    free_gb: 10.0,
    label: "Backblaze B2",
};

/// Source : cloudflare.com/plans, vérifié 2026-08-16 — le CDN du plan
/// gratuit n'est pas mesuré (pas de coût par requête), contrairement aux
/// Workers (compute) qui eux le sont. C'est le seul des 3 ajouts hybrides
/// du diagramme "Venice Deployment" qui reste à 0€ à *tout* volume
/// réaliste pour ce projet, pas seulement à volume zéro.
pub const CDN_MONTHLY_EUR: f64 = 0.0;

/// GitHub Container Registry (ghcr.io/retelabs, depuis la migration du
/// 2026-09-30). Vérifié sur docs.github.com (billing, GitHub Packages) :
/// gratuit pour les paquets publics ; en privé, le plan Free d'organisation
/// inclut 500 Mo de stockage et 1 Go de transfert par mois, bloqué au-delà
/// sans moyen de paiement. La CI ne pousse une image que sur un tag `v*` ou à
/// la main, ce qui reste dans ce quota — d'où 0€.
pub const CONTAINER_REGISTRY_MONTHLY_EUR: f64 = 0.0;

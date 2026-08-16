//! Le calcul lui-même — fonctions pures, aucune dépendance à Docker ou
//! ClickHouse, pour rester testables sans rien de réel en marche. Les
//! entrées mesurées/vérifiées (`measure.rs`, `pricing.rs`) sont passées en
//! paramètres, pas relues ici.

use crate::pricing::{ObjectStoragePricing, VmPricing};

const BYTES_PER_GB: f64 = 1_000_000_000.0; // Go décimal — l'unité que les fournisseurs cotent.

pub struct CostReport {
    pub spans_per_day: u64,
    pub retention_days: u32,
    /// Volume stocké à l'état stationnaire (la fenêtre de rétention est
    /// pleine) — pas "aujourd'hui", ce que la table pèsera une fois que
    /// le TTL a atteint son régime permanent.
    pub stored_spans: u64,
    pub stored_gb: f64,
    /// Combien de jours à ce volume avant de dépasser le disque inclus de
    /// la VM, en supposant une croissance linéaire depuis un disque vide —
    /// `None` à volume nul (la croissance ne se produit jamais).
    pub days_until_disk_full: Option<f64>,
    /// Si `stored_gb` (l'état stationnaire) dépasse le disque inclus —
    /// distinct de `days_until_disk_full` : la rétention peut plafonner la
    /// croissance avant que le disque ne se remplisse, ou au contraire le
    /// disque peut se remplir avant que la rétention n'ait jamais l'effet
    /// de plafonner quoi que ce soit.
    pub steady_state_exceeds_disk: bool,
    pub vm_monthly_eur: f64,
    pub object_storage_monthly_usd: f64,
    pub cdn_monthly_eur: f64,
    pub registry_monthly_eur: f64,
}

pub fn compute(
    spans_per_day: u64,
    bytes_per_span: f64,
    retention_days: u32,
    vm: &VmPricing,
    storage: &ObjectStoragePricing,
) -> CostReport {
    let stored_spans = spans_per_day * u64::from(retention_days);
    let stored_gb = (stored_spans as f64 * bytes_per_span) / BYTES_PER_GB;

    let disk_bytes = vm.included_disk_gb * BYTES_PER_GB;
    let days_until_disk_full = if spans_per_day == 0 {
        None
    } else {
        Some(disk_bytes / (spans_per_day as f64 * bytes_per_span))
    };

    let billable_storage_gb = (stored_gb - storage.free_gb).max(0.0);

    CostReport {
        spans_per_day,
        retention_days,
        stored_spans,
        stored_gb,
        days_until_disk_full,
        steady_state_exceeds_disk: stored_gb * BYTES_PER_GB > disk_bytes,
        vm_monthly_eur: vm.monthly_eur,
        object_storage_monthly_usd: billable_storage_gb * storage.per_gb_month_usd,
        // Ces deux-là sont des constantes vérifiées (docs/cost-model.md),
        // pas des fonctions du volume — Cloudflare (CDN) et GitLab
        // (registre) sont déjà à 0€ à tout volume réaliste pour ce projet.
        cdn_monthly_eur: crate::pricing::CDN_MONTHLY_EUR,
        registry_monthly_eur: crate::pricing::CONTAINER_REGISTRY_MONTHLY_EUR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_vm() -> VmPricing {
        VmPricing {
            monthly_eur: 5.49,
            included_disk_gb: 40.0,
            label: "test-vm",
        }
    }

    fn test_storage() -> ObjectStoragePricing {
        ObjectStoragePricing {
            per_gb_month_usd: 0.00695,
            free_gb: 10.0,
            label: "test-storage",
        }
    }

    #[test]
    fn zero_volume_means_zero_hybrid_extras() {
        let report = compute(0, 300.0, 90, &test_vm(), &test_storage());
        assert_eq!(report.stored_spans, 0);
        assert_eq!(report.stored_gb, 0.0);
        assert_eq!(report.object_storage_monthly_usd, 0.0);
        assert_eq!(report.days_until_disk_full, None);
        // The VM itself is not zero — it's the one cost that's identical
        // between "100% perso" and "hybride", not something the hybrid
        // extras add.
        assert_eq!(report.vm_monthly_eur, 5.49);
    }

    #[test]
    fn storage_cost_grows_with_volume() {
        let low = compute(1_000, 300.0, 90, &test_vm(), &test_storage());
        let high = compute(1_000_000, 300.0, 90, &test_vm(), &test_storage());
        assert!(high.stored_gb > low.stored_gb);
        assert!(high.object_storage_monthly_usd > low.object_storage_monthly_usd);
    }

    #[test]
    fn storage_under_the_free_tier_costs_nothing() {
        // 100 spans/day * 90 days * 300 bytes ≈ 0.0027 GB, way under the
        // 10 GB Backblaze free tier.
        let report = compute(100, 300.0, 90, &test_vm(), &test_storage());
        assert_eq!(report.object_storage_monthly_usd, 0.0);
    }

    #[test]
    fn a_high_enough_volume_would_outgrow_the_included_disk() {
        // 10M spans/day * 300 bytes = 3 GB/day; over the 90-day retention
        // window that's 270 GB, well past the 40 GB included disk.
        let report = compute(10_000_000, 300.0, 90, &test_vm(), &test_storage());
        assert!(report.steady_state_exceeds_disk);
        let days = report.days_until_disk_full.unwrap();
        assert!(
            days < 90.0,
            "expected disk to fill before retention caps growth, got {days} days"
        );
    }

    #[test]
    fn a_low_enough_volume_fits_comfortably_within_retention() {
        let report = compute(1_000, 300.0, 90, &test_vm(), &test_storage());
        assert!(!report.steady_state_exceeds_disk);
    }
}

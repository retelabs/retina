//! Static onboarding/help content — shown both in the paginated intro
//! (`main.rs::show_intro`) and in the in-app `View::Help` tab (`ui.rs`),
//! so there's exactly one place that knows this text, not two copies that
//! could drift apart.
//!
//! Every claim here is grounded in the project's own real history
//! (`CLAUDE.md`), not invented for the occasion — dates, real client names,
//! real doc paths.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

pub struct Page {
    pub title: &'static str,
    pub body: Vec<Line<'static>>,
}

fn line(text: &'static str) -> Line<'static> {
    Line::from(text)
}

fn heading(text: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        text,
        Style::default().add_modifier(Modifier::BOLD),
    ))
}

pub fn pages() -> Vec<Page> {
    vec![
        Page {
            title: "Venice",
            body: vec![
                line("Kernel Rust d'observabilité pour workflows d'agents LLM,"),
                line("inspiré d'OpenTelemetry et Datadog."),
                Line::default(),
                line("Ingestion OTLP/gRPC standard — n'importe quel SDK OpenTelemetry"),
                line("envoie des traces sans code custom côté client."),
                Line::default(),
                line("Des plugins métiers interprètent des invariants réels posés"),
                line("comme attributs sur les spans (conformité, fraude, éval) —"),
                line("pas un dashboard générique, un kernel qui comprend le métier."),
            ],
        },
        Page {
            title: "Histoire",
            body: vec![
                line("Anciennement \"trellis\" — renommé Venice le 2026-08-17."),
                Line::default(),
                line("Venise : ville connue pour ses canaux, choisie pour son écho"),
                line("avec l'architecture réelle du kernel — un pipeline principal"),
                line("(ingestion -> plugins -> stockage) avec des embranchements qui"),
                line("se greffent dessus sans le bloquer, comme un réseau de canaux"),
                line("interconnectés plutôt qu'un canal unique."),
                Line::default(),
                line("Auto-hébergé par choix (ADR-0002) : l'objectif n'est pas de"),
                line("consommer des services cloud managés, mais de comprendre —"),
                line("et coder soi-même — les briques d'infrastructure qu'ils"),
                line("remplacent d'habitude."),
            ],
        },
        Page {
            title: "Écosystème",
            body: vec![
                heading("Vérifié en conditions réelles, pas en théorie :"),
                Line::default(),
                line("client-project (the-client) — SaaS santé, agents triage/résumé/"),
                line("conformité/enrichissement d'appel. Coût $ par span et éval"),
                line("déterministe (dérive de vocabulaire triage) vérifiés en"),
                line("production, données réelles visibles dans ce TUI."),
                Line::default(),
                line("aws_factory (fraudos) — détection de fraude, rejoué en gRPC"),
                line("réel contre le kernel (crates/fraudos-replay)."),
                Line::default(),
                line("Un plugin s'écrit pour n'importe quel autre vertical : lire"),
                line("les attributs `<vertical>.*` d'un span, produire des"),
                line("warnings/attributs dérivés (crates/plugin-api)."),
            ],
        },
        Page {
            title: "Documentation",
            body: vec![
                heading("Pour intégrer une application cliente :"),
                line("  docs/client-integration.md"),
                Line::default(),
                heading("Contrats techniques vérifiés (OTLP, ClickHouse, auth...) :"),
                line("  docs/interfaces/"),
                Line::default(),
                heading("Décisions d'architecture (ADR) :"),
                line("  docs/adr/"),
                Line::default(),
                heading("Vue d'ensemble et démarrage rapide :"),
                line("  README.md"),
                Line::default(),
                line("Cette page reste accessible depuis l'app — onglet \"Aide\"."),
            ],
        },
    ]
}

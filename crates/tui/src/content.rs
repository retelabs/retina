//! Static onboarding/help content — shown both in the paginated intro
//! (`main.rs::show_intro`) and in the in-app `View::Help` tab (`ui.rs`),
//! so there's exactly one place that knows this text, not two copies that
//! could drift apart.
//!
//! Product-facing copy: what Venice is, what it does, how to use it —
//! not an engineering changelog. Internal history (the trellis rename,
//! verification details) belongs in `CLAUDE.md`, not here.

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
                line("Observability for agentic LLM workflows."),
                Line::default(),
                line("Send traces from any OpenTelemetry SDK — zero custom code."),
                line("Venice ingests them, understands your business logic through"),
                line("plugins, and stores everything so you can query it later."),
                Line::default(),
                line("Not a generic dashboard: a kernel that reads what your agents"),
                line("actually did, not just how long it took."),
            ],
        },
        Page {
            title: "What it does",
            body: vec![
                heading("Ingest"),
                line("  Standard OTLP/gRPC. If you already use OpenTelemetry, you're"),
                line("  already compatible."),
                Line::default(),
                heading("Understand"),
                line("  Plugins read the attributes on your spans and apply real"),
                line("  business rules — compliance gates, fraud signals, data"),
                line("  quality checks — and surface warnings you can act on."),
                Line::default(),
                heading("Track cost"),
                line("  Token usage and $ cost per call, computed automatically"),
                line("  across providers and models."),
                Line::default(),
                heading("Query"),
                line("  A simple HTTP API, or this terminal dashboard — traces,"),
                line("  drill-down, and aggregate metrics."),
            ],
        },
        Page {
            title: "Get started",
            body: vec![
                line("Point any OpenTelemetry SDK at the kernel's gRPC endpoint —"),
                line("no Venice-specific code required."),
                Line::default(),
                line("This dashboard mirrors the query API: browse recent traces,"),
                line("drill into a trace's span tree, or check the metrics summary."),
                Line::default(),
                heading("Official documentation"),
                line("  README.md — overview and quick start"),
                line("  docs/client-integration.md — full integration guide"),
                Line::default(),
                line("This page is always one Tab away — look for \"Help\"."),
            ],
        },
    ]
}

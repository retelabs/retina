//! Static onboarding/reference content.
//!
//! Two audiences, two page lists, built from the same small set of page
//! functions so there's still exactly one place that owns each page's
//! text:
//! - `intro_pages()` — the full onboarding tour (`main.rs::show_intro`),
//!   shown once at startup: what Venice is, what it does, how to start.
//! - `reference_pages()` — the in-app `View::Help` tab (`ui.rs`), reachable
//!   any time via `Tab`: just the `man`-style quick reference (commands,
//!   glossary), not the onboarding narrative — that's a one-time tour, not
//!   something to re-read while using the app.
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

/// A `man`-style two-column entry: bold key, left-padded, plain
/// description — built as an owned `String` (not `&'static str`) since the
/// padding width depends on the longest key in its table, computed once per
/// table rather than hand-aligned per line.
fn kv(key: &str, key_width: usize, desc: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {key:<key_width$}"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(desc.to_string()),
    ])
}

fn venice_page() -> Page {
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
    }
}

fn what_it_does_page() -> Page {
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
    }
}

fn get_started_page() -> Page {
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
            line("Command reference and glossary are always one Tab away —"),
            line("look for \"Help\"."),
        ],
    }
}

fn commands_page() -> Page {
    let w = 12;
    Page {
        title: "Commands",
        body: vec![
            heading("NAVIGATION"),
            kv("Tab", w, "switch view (Traces / Detail / Metrics / Help)"),
            kv("↑/↓ k/j", w, "move selection (Traces view)"),
            kv("←/→", w, "previous / next page (Help view)"),
            kv("Enter", w, "open trace detail (Traces) · continue (intro)"),
            kv("Esc", w, "back to Traces (Detail) · quit (elsewhere)"),
            kv("q", w, "quit"),
            Line::default(),
            heading("ACTIONS"),
            kv("r", w, "refresh the current view"),
        ],
    }
}

fn glossary_page() -> Page {
    let w = 16;
    Page {
        title: "Glossary",
        body: vec![
            kv(
                "trace",
                w,
                "a group of spans sharing one trace_id — one run",
            ),
            kv("span", w, "one recorded unit of work"),
            kv("agent_run", w, "a span for one LLM agent invocation"),
            kv("model_call", w, "a span for one call to a model / provider"),
            kv("tool_call", w, "a span for one tool call made by an agent"),
            Line::default(),
            kv(
                "cost_usd",
                w,
                "computed $ cost from token usage and pricing —",
            ),
            line("              null if the model isn't priced yet"),
            kv("warning", w, "a signal a plugin attached to a span, worth"),
            line("              investigating (see the Metrics summary)"),
        ],
    }
}

/// The full startup tour: product pitch, then the same reference pages
/// the in-app "Help" tab offers — read once, end to end.
pub fn intro_pages() -> Vec<Page> {
    vec![
        venice_page(),
        what_it_does_page(),
        get_started_page(),
        commands_page(),
        glossary_page(),
    ]
}

/// The in-app "Help" tab: quick reference only, not the onboarding
/// narrative — that's a one-time tour (`intro_pages`), not something to
/// re-read while actively using the app.
pub fn reference_pages() -> Vec<Page> {
    vec![commands_page(), glossary_page()]
}

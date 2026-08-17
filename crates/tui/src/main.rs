//! `venice-tui` — terminal dashboard for `query-api`, the strict mirror of
//! its 3 endpoints (traces list / trace detail / metrics summary), nothing
//! query-api doesn't already expose. Reuses `query_api::dto` directly
//! (`crates/tui/src/api.rs`) instead of a second copy of the wire shape.

use crossterm::event::{self, Event, KeyCode, KeyEventKind};

use tui::api::ApiClient;
use tui::app::{App, View};
use tui::ui;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

async fn refresh_traces(client: &ApiClient, app: &mut App) {
    match client.list_traces(50).await {
        Ok(traces) => {
            app.traces = traces;
            app.selected_trace = app.selected_trace.min(app.traces.len().saturating_sub(1));
            app.status = None;
        }
        Err(e) => app.status = Some(format!("error /traces: {e}")),
    }
}

async fn refresh_trace_detail(client: &ApiClient, app: &mut App) {
    let Some(trace) = app.traces.get(app.selected_trace) else {
        return;
    };
    match client.get_trace(&trace.trace_id).await {
        Ok(spans) => {
            app.trace_spans = spans;
            app.status = None;
        }
        Err(e) => app.status = Some(format!("error /traces/{{id}}: {e}")),
    }
}

async fn refresh_metrics(client: &ApiClient, app: &mut App) {
    match client.metrics_summary().await {
        Ok(metrics) => {
            app.metrics = Some(metrics);
            app.status = None;
        }
        Err(e) => app.status = Some(format!("error /metrics/summary: {e}")),
    }
}

/// Paginated intro: screen 0 is the logo splash (`ui::draw_splash`), screens
/// 1.. are `content::pages()` (the same pages the in-app "Help" tab shows —
/// `content.rs` is the one place that owns this text). Fully manual
/// navigation, no auto-advance timer: forcing a fixed delay while someone
/// is actually reading multi-page content would fight the point of making
/// this "interactive" rather than a fixed-duration splash.
async fn show_intro(
    terminal: &mut ratatui::DefaultTerminal,
) -> Result<(), Box<dyn std::error::Error>> {
    // `None` if decoding the real logo failed for any reason — draw_splash
    // falls back to the computed ASCII badge rather than the whole TUI
    // refusing to start over a missing image. Sized against the real
    // terminal (not a fixed guess): the ASCII art is sampled at a fixed
    // resolution up front, not resized after the fact, so picking a size
    // that doesn't fit *before* sampling is exactly the bug that made the
    // logo not render at all on a common 80×24 terminal (crates/tui/src/logo.rs).
    let logo = tui::logo::load(terminal.size()?);
    let pages = tui::content::pages();
    let total_screens = 1 + pages.len();
    let mut screen: usize = 0;

    loop {
        terminal.draw(|frame| {
            if screen == 0 {
                ui::draw_splash(frame, logo.as_ref());
            } else {
                let is_last = screen + 1 >= total_screens;
                ui::draw_intro_page(frame, &pages[screen - 1], screen - 1, pages.len(), is_last);
            }
        })?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(()), // skip the whole intro
            KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Down => {
                if screen + 1 >= total_screens {
                    return Ok(()); // last screen, "next" enters the app
                }
                screen += 1;
            }
            KeyCode::Left | KeyCode::Up => screen = screen.saturating_sub(1),
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base_url = env_or("QUERY_API_URL", "http://localhost:8080");
    let api_key = std::env::var("QUERY_API_KEY")
        .map_err(|_| "QUERY_API_KEY must be set — see docs/interfaces/kernel-auth.md")?;
    let client = ApiClient::new(base_url, api_key);
    let mut app = App::new();

    let mut terminal = ratatui::init();
    let result = async {
        show_intro(&mut terminal).await?;
        refresh_traces(&client, &mut app).await;
        run(&mut terminal, &client, &mut app).await
    }
    .await;
    ratatui::restore();
    result
}

async fn run(
    terminal: &mut ratatui::DefaultTerminal,
    client: &ApiClient,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    // Same content the paginated intro shows — reachable any time via the
    // "Help" tab, not just at startup (the whole point of the request that
    // led to this: docs shouldn't only exist as a one-shot splash).
    let help_page_count = tui::content::pages().len();

    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;

        if !event::poll(std::time::Duration::from_millis(200))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc if app.view != View::TraceDetail => {
                app.should_quit = true;
            }
            KeyCode::Esc => app.view = View::Traces,
            KeyCode::Tab => {
                app.view = match app.view {
                    View::Traces => View::TraceDetail,
                    View::TraceDetail => View::Metrics,
                    View::Metrics => View::Help,
                    View::Help => View::Traces,
                };
                if app.view == View::Metrics && app.metrics.is_none() {
                    refresh_metrics(client, app).await;
                }
            }
            KeyCode::Down | KeyCode::Char('j') if app.view == View::Traces => app.select_next(),
            KeyCode::Up | KeyCode::Char('k') if app.view == View::Traces => app.select_prev(),
            KeyCode::Right if app.view == View::Help => app.help_next_page(help_page_count),
            KeyCode::Left if app.view == View::Help => app.help_prev_page(),
            KeyCode::Enter if app.view == View::Traces => {
                app.view = View::TraceDetail;
                refresh_trace_detail(client, app).await;
            }
            KeyCode::Char('r') => match app.view {
                View::Traces => refresh_traces(client, app).await,
                View::TraceDetail => refresh_trace_detail(client, app).await,
                View::Metrics => refresh_metrics(client, app).await,
                View::Help => {}
            },
            _ => {}
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

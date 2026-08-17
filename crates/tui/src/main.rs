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
        Err(e) => app.status = Some(format!("erreur /traces: {e}")),
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
        Err(e) => app.status = Some(format!("erreur /traces/{{id}}: {e}")),
    }
}

async fn refresh_metrics(client: &ApiClient, app: &mut App) {
    match client.metrics_summary().await {
        Ok(metrics) => {
            app.metrics = Some(metrics);
            app.status = None;
        }
        Err(e) => app.status = Some(format!("erreur /metrics/summary: {e}")),
    }
}

/// Shown for a fixed duration or until any key is pressed — a splash isn't
/// worth making someone wait through, so any key skips it rather than
/// forcing the full duration.
const SPLASH_DURATION: std::time::Duration = std::time::Duration::from_millis(4000);

async fn show_splash(
    terminal: &mut ratatui::DefaultTerminal,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    loop {
        terminal.draw(ui::draw_splash)?;
        if start.elapsed() >= SPLASH_DURATION {
            return Ok(());
        }
        if event::poll(std::time::Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            return Ok(());
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
        show_splash(&mut terminal).await?;
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
                    View::Metrics => View::Traces,
                };
                if app.view == View::Metrics && app.metrics.is_none() {
                    refresh_metrics(client, app).await;
                }
            }
            KeyCode::Down | KeyCode::Char('j') if app.view == View::Traces => app.select_next(),
            KeyCode::Up | KeyCode::Char('k') if app.view == View::Traces => app.select_prev(),
            KeyCode::Enter if app.view == View::Traces => {
                app.view = View::TraceDetail;
                refresh_trace_detail(client, app).await;
            }
            KeyCode::Char('r') => match app.view {
                View::Traces => refresh_traces(client, app).await,
                View::TraceDetail => refresh_trace_detail(client, app).await,
                View::Metrics => refresh_metrics(client, app).await,
            },
            _ => {}
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

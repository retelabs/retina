//! Rendering — one function per view, kept separate from `app.rs` (state)
//! and `main.rs` (event loop) so each stays about one thing.

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs};

use crate::app::{App, View, humanize_ago, span_tree};

fn now_unix_nano() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(frame.area());

    draw_tabs(frame, chunks[0], app.view);

    match app.view {
        View::Traces => draw_traces(frame, chunks[1], app),
        View::TraceDetail => draw_trace_detail(frame, chunks[1], app),
        View::Metrics => draw_metrics(frame, chunks[1], app),
    }

    draw_footer(frame, chunks[2], app);
}

fn draw_tabs(frame: &mut Frame, area: Rect, current: View) {
    let titles = ["Traces", "Détail", "Métriques"];
    let selected = match current {
        View::Traces => 0,
        View::TraceDetail => 1,
        View::Metrics => 2,
    };
    let tabs = Tabs::new(titles.to_vec())
        .block(Block::default().borders(Borders::ALL).title(" Venice "))
        .select(selected)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, area);
}

fn draw_traces(frame: &mut Frame, area: Rect, app: &App) {
    let now = now_unix_nano();
    let items: Vec<ListItem> = app
        .traces
        .iter()
        .map(|t| {
            let ago = humanize_ago(t.start_time_unix_nano, now);
            let line = format!(
                "{}  ·  {} spans  ·  il y a {ago}",
                &t.trace_id[..t.trace_id.len().min(16)],
                t.span_count
            );
            ListItem::new(line)
        })
        .collect();

    let mut state = ListState::default();
    if !app.traces.is_empty() {
        state.select(Some(app.selected_trace));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Traces récentes (↑/↓, Entrée pour le détail, r pour rafraîchir) "),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_trace_detail(frame: &mut Frame, area: Rect, app: &App) {
    let tree = span_tree(&app.trace_spans);
    let lines: Vec<Line> = tree
        .iter()
        .map(|(depth, s)| {
            let indent = "  ".repeat(*depth);
            let cost = s
                .cost_usd
                .map(|c| format!(" · ${c:.6}"))
                .unwrap_or_default();
            let warning = if s.extra_attributes.contains_key("plugin.warning") {
                " ⚠"
            } else {
                ""
            };
            let label = s
                .agent_name
                .as_deref()
                .or(s.tool_name.as_deref())
                .unwrap_or(s.operation_name.as_str());
            Line::from(format!(
                "{indent}{} [{}] {label}{cost}{warning}",
                s.kind, s.status_code,
            ))
        })
        .collect();

    let title = format!(
        " Trace {} — {} spans (Échap pour revenir) ",
        app.trace_spans
            .first()
            .map(|s| s.trace_id.as_str())
            .unwrap_or(""),
        app.trace_spans.len()
    );

    let paragraph =
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(paragraph, area);
}

fn draw_metrics(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines: Vec<Line> = Vec::new();

    match &app.metrics {
        None => lines.push(Line::from("chargement...")),
        Some(metrics) => {
            for kind in &metrics.by_kind {
                let cost = kind
                    .total_cost_usd
                    .map(|c| format!("${c:.4}"))
                    .unwrap_or_else(|| "—".to_string());
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<12}", kind.kind),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!(
                        "  spans={}  input_tokens={}  output_tokens={}  cost={cost}",
                        kind.span_count, kind.total_input_tokens, kind.total_output_tokens
                    )),
                ]));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(format!(
                "spans_with_warnings: {}",
                metrics.spans_with_warnings
            )));
        }
    }

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Résumé (r pour rafraîchir) "),
    );
    frame.render_widget(paragraph, area);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let text = app
        .status
        .clone()
        .unwrap_or_else(|| "Tab: changer de vue · q: quitter".to_string());
    frame.render_widget(Paragraph::new(text), area);
}

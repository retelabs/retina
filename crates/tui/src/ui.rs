//! Rendering — one function per view, kept separate from `app.rs` (state)
//! and `main.rs` (event loop) so each stays about one thing.

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs};

use crate::app::{App, View, humanize_ago, span_tree};
use crate::content;

/// The violet of the Retina mark (`#8B7CF8`, `UI/assets/logos/`) — an
/// `Rgb` value, so it only renders as true violet on a truecolor terminal;
/// degrades to the nearest ANSI color elsewhere rather than failing.
const RETINA_VIOLET: Color = Color::Rgb(139, 124, 248);

fn now_unix_nano() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// A `width × height` character canvas: lets the badge be composed in
/// layers (orbit, then the spectrum bars, then the pupil on top, so later
/// layers paint over earlier ones like the real logo's z-order) instead of
/// computing one flat pattern in a single pass.
struct Canvas {
    cells: Vec<Vec<char>>,
    width: i32,
    height: i32,
}

impl Canvas {
    fn new(width: i32, height: i32) -> Self {
        Self {
            cells: vec![vec![' '; width.max(0) as usize]; height.max(0) as usize],
            width,
            height,
        }
    }

    fn set(&mut self, row: i32, col: i32, ch: char) {
        if row >= 0 && row < self.height && col >= 0 && col < self.width {
            self.cells[row as usize][col as usize] = ch;
        }
    }

    fn into_lines(self) -> Vec<Line<'static>> {
        self.cells
            .into_iter()
            .map(|row| {
                let text: String = row.into_iter().collect();
                Line::from(Span::styled(text, Style::default().fg(RETINA_VIOLET)))
                    .alignment(Alignment::Center)
            })
            .collect()
    }
}

/// Fallback badge, drawn only when the embedded logo fails to decode: the
/// Retina spectral iris (`UI/assets/logos/generate.py`) rebuilt in text. A
/// tilted orbit behind, a ring of spectrum bars whose lengths follow the same
/// harmonic profile as the real mark, and a star for a pupil. Every element
/// is computed from row/column arithmetic (polar coordinates, an ellipse)
/// rather than typed by eye, so proportions hold at any `height`.
fn retina_badge_lines(height: u16) -> Vec<Line<'static>> {
    let h = (height as i32).max(10);
    // Terminal cells are roughly twice as tall as they are wide: x distances
    // are doubled so the iris renders round rather than as a tall oval.
    let aspect = 2.0;
    let radius = (h as f64 - 1.0) / 2.0;
    let width = (radius * aspect * 2.0 * 1.3).round() as i32 + 1;
    let cx = width / 2;
    let cy = h / 2;
    let mut canvas = Canvas::new(width, h);
    let plot = |canvas: &mut Canvas, x: f64, y: f64, ch: char| {
        canvas.set(
            (cy as f64 + y).round() as i32,
            (cx as f64 + x * aspect).round() as i32,
            ch,
        );
    };

    // The orbit, drawn first so the bars paint over it: an ellipse tilted by
    // about 24 degrees, like the real mark's.
    let (tilt_cos, tilt_sin) = ((-0.42_f64).cos(), (-0.42_f64).sin());
    let (orbit_rx, orbit_ry) = (radius * 1.25, radius * 0.4);
    let orbit_point = |t: f64| {
        let (ex, ey) = (orbit_rx * t.cos(), orbit_ry * t.sin());
        (ex * tilt_cos - ey * tilt_sin, ex * tilt_sin + ey * tilt_cos)
    };
    for i in 0..120 {
        let (x, y) = orbit_point(i as f64 / 120.0 * std::f64::consts::TAU);
        plot(&mut canvas, x, y, '·');
    }

    // The spectrum ring: bars from an inner radius outwards, each drawn with
    // the box character closest to its direction.
    let bars = 24;
    let inner = radius * 0.35;
    for i in 0..bars {
        let a = std::f64::consts::TAU * i as f64 / bars as f64 - std::f64::consts::FRAC_PI_2;
        let amp = 0.55
            + 0.25 * (3.0 * a).sin()
            + 0.15 * (7.0 * a + 1.3).sin()
            + 0.08 * (13.0 * a + 0.4).sin();
        let outer = inner + radius * (0.2 + 0.45 * amp);
        let (dx, dy) = (a.cos(), a.sin());
        let glyph = match ((dy.atan2(dx).to_degrees() + 360.0) % 180.0) as i32 {
            0..=22 | 158..=180 => '─',
            23..=67 => '╲',
            68..=112 => '│',
            _ => '╱',
        };
        let steps = ((outer - inner) * 2.0).ceil() as i32;
        for s in 0..=steps {
            let r = inner + (outer - inner) * s as f64 / steps.max(1) as f64;
            plot(&mut canvas, r * dx, r * dy, glyph);
        }
    }

    // The moon on the orbit's front half, then the pupil.
    let (mx, my) = orbit_point(0.55);
    plot(&mut canvas, mx, my, '●');
    plot(&mut canvas, 0.0, 0.0, '✦');
    for (x, y) in [(-0.5, 0.0), (0.5, 0.0), (0.0, -1.0), (0.0, 1.0)] {
        plot(&mut canvas, x, y, '·');
    }

    canvas.into_lines()
}

fn splash_caption_lines() -> Vec<Line<'static>> {
    vec![
        Line::default(),
        Line::from(Span::styled(
            "R E T I N A",
            Style::default()
                .fg(RETINA_VIOLET)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(Span::styled(
            "agentic observability kernel",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ))
        .alignment(Alignment::Center),
        Line::default(),
        Line::from(Span::styled(
            "→ to learn more · Esc to skip",
            Style::default().fg(Color::DarkGray),
        ))
        .alignment(Alignment::Center),
    ]
}

/// Centers a `width × height` area inside `outer` — the logo's `Lines` are
/// already sampled at an exact `width`/`height`, and a `Paragraph` doesn't
/// center a smaller block inside a larger area on its own.
fn centered_rect(outer: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(outer.width);
    let height = height.min(outer.height);
    Rect {
        x: outer.x + (outer.width.saturating_sub(width)) / 2,
        y: outer.y + (outer.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

/// `logo` is `None` when `crate::logo::load()` failed (decode error,
/// unexpected font metrics) — falls back to the computed ASCII badge
/// (`retina_badge_lines`) rather than showing a blank gap where the real
/// image would have been.
pub fn draw_splash(frame: &mut Frame, logo: Option<&crate::logo::Logo>) {
    let area = frame.area();
    let caption = splash_caption_lines();
    let caption_height = caption.len() as u16;

    match logo {
        Some(logo) => {
            let vchunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Fill(1),
                    Constraint::Length(logo.cells.height),
                    Constraint::Length(caption_height),
                    Constraint::Fill(1),
                ])
                .split(area);
            let image_area = centered_rect(vchunks[1], logo.cells.width, logo.cells.height);
            frame.render_widget(Paragraph::new(logo.lines.clone()), image_area);
            frame.render_widget(Paragraph::new(caption), vchunks[2]);
        }
        None => {
            let badge_height = area.height.saturating_sub(10).clamp(10, 18);
            let mut lines = retina_badge_lines(badge_height);
            lines.extend(caption);
            let content_height = lines.len() as u16;
            let vchunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Fill(1),
                    Constraint::Length(content_height),
                    Constraint::Fill(1),
                ])
                .split(area);
            frame.render_widget(Paragraph::new(lines), vchunks[1]);
        }
    }
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
        View::Help => draw_help(frame, chunks[1], app),
    }

    draw_footer(frame, chunks[2], app);
}

/// Shared border style so every panel reads as one app, not a grab-bag of
/// default-white ratatui boxes.
fn retina_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(RETINA_VIOLET))
        .title(Span::styled(
            title,
            Style::default()
                .fg(RETINA_VIOLET)
                .add_modifier(Modifier::BOLD),
        ))
}

fn draw_tabs(frame: &mut Frame, area: Rect, current: View) {
    let titles = ["Traces", "Detail", "Metrics", "Help"];
    let selected = match current {
        View::Traces => 0,
        View::TraceDetail => 1,
        View::Metrics => 2,
        View::Help => 3,
    };
    let tabs = Tabs::new(titles.to_vec())
        .block(retina_block(" Retina "))
        .select(selected)
        .highlight_style(
            Style::default()
                .fg(RETINA_VIOLET)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
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
                "{}  ·  {} spans  ·  {ago} ago",
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
        .block(retina_block(
            " Recent traces (↑/↓, Enter for detail, r to refresh) ",
        ))
        .highlight_style(
            Style::default()
                .bg(RETINA_VIOLET)
                .fg(Color::Black)
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
        " Trace {} — {} spans (Esc to go back) ",
        app.trace_spans
            .first()
            .map(|s| s.trace_id.as_str())
            .unwrap_or(""),
        app.trace_spans.len()
    );

    let paragraph = Paragraph::new(lines).block(retina_block(&title));
    frame.render_widget(paragraph, area);
}

fn draw_metrics(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines: Vec<Line> = Vec::new();

    match &app.metrics {
        None => lines.push(Line::from("loading...")),
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
                            .fg(RETINA_VIOLET)
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

    let paragraph = Paragraph::new(lines).block(retina_block(" Summary (r to refresh) "));
    frame.render_widget(paragraph, area);
}

/// Shared by `draw_help` (in-app, via the "Help" tab) and the paginated
/// intro (`main.rs::show_intro`, full-screen instead of inside the tab
/// layout) — one rendering of a `content::Page`, not two.
pub fn draw_content_page(
    frame: &mut Frame,
    area: Rect,
    page: &content::Page,
    index: usize,
    count: usize,
) {
    let title = format!(" {} ({}/{}) ", page.title, index + 1, count);
    let paragraph = Paragraph::new(page.body.clone()).block(retina_block(&title));
    frame.render_widget(paragraph, area);
}

/// Intro-only: the content page plus a one-line navigation hint below it —
/// the in-app "Help" tab already gets its hint from `draw_footer`'s normal
/// status line, but the intro has no such chrome of its own.
pub fn draw_intro_page(
    frame: &mut Frame,
    page: &content::Page,
    index: usize,
    count: usize,
    is_last: bool,
) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);
    draw_content_page(frame, chunks[0], page, index, count);

    let hint = if is_last {
        "← previous page · Enter: open the app · Esc: skip"
    } else {
        "←/→: previous/next page · Esc: skip to the app"
    };
    frame.render_widget(
        Paragraph::new(hint)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center),
        chunks[1],
    );
}

fn draw_help(frame: &mut Frame, area: Rect, app: &App) {
    let pages = content::reference_pages();
    let page = &pages[app.help_page.min(pages.len() - 1)];
    draw_content_page(frame, area, page, app.help_page, pages.len());
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let default_hint = match app.view {
        View::Help => "←/→: previous/next page · Tab: switch view · q: quit",
        _ => "Tab: switch view · q: quit",
    };
    let text = app
        .status
        .clone()
        .unwrap_or_else(|| default_hint.to_string());
    frame.render_widget(Paragraph::new(text), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[Line<'static>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn the_badge_has_the_requested_height_and_even_rows() {
        for height in [10u16, 14, 18] {
            let rows = text(&retina_badge_lines(height));
            assert_eq!(rows.len(), height as usize);
            let widths: Vec<usize> = rows.iter().map(|r| r.chars().count()).collect();
            assert!(widths.windows(2).all(|w| w[0] == w[1]), "{widths:?}");
        }
    }

    #[test]
    fn the_badge_draws_the_iris_not_the_old_v() {
        let rows = text(&retina_badge_lines(14)).join("\n");
        assert!(rows.contains('✦'), "pupil missing:\n{rows}");
        assert!(
            rows.contains('│') && rows.contains('─'),
            "bars missing:\n{rows}"
        );
        assert!(!rows.contains('█'), "the old V is still drawn:\n{rows}");
    }

    #[test]
    fn the_splash_names_retina() {
        let caption = text(&splash_caption_lines()).join("\n");
        assert!(caption.contains("R E T I N A"), "{caption}");
    }
}

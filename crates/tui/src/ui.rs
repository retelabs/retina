//! Rendering — one function per view, kept separate from `app.rs` (state)
//! and `main.rs` (event loop) so each stays about one thing.

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs};

use crate::app::{App, View, humanize_ago, span_tree};

/// Approximates the teal in `UI/assets/logos/logo_venice_v1.png` — an
/// `Rgb` value, so it only renders as true teal on a truecolor terminal;
/// degrades to the nearest ANSI color elsewhere rather than failing.
const VENICE_TEAL: Color = Color::Rgb(15, 110, 110);

fn now_unix_nano() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// A `width × height` character canvas — lets the badge be composed in
/// layers (ring, then interior canal lines, then the bold `V` on top, so
/// later layers paint over earlier ones exactly like the real logo's
/// z-order) instead of computing one flat pattern in a single pass.
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
                Line::from(Span::styled(text, Style::default().fg(VENICE_TEAL)))
                    .alignment(Alignment::Center)
            })
            .collect()
    }
}

/// Stylized ASCII rendition of `UI/assets/logos/logo_venice_v1.png` — same
/// compositional elements (ring, 4 corner nodes, an interior lattice of
/// thin canal lines with node dots, a bold `V` with a small tail at its
/// point), not a pixel-identical reproduction: the source PNG's lattice is
/// organic/hand-varied linework, which doesn't have a single "correct"
/// parametric form to reproduce exactly in monospace text. Every element
/// here is computed from row/column arithmetic (circle equation, line
/// interpolation) rather than typed by eye, so proportions stay correct at
/// any `height` instead of only looking right at whichever size it was
/// eyeballed against.
fn venice_badge_lines(height: u16) -> Vec<Line<'static>> {
    let h = (height as i32).max(10);
    let radius_y = h as f64 / 2.0;
    // Terminal character cells are roughly twice as tall as they are wide —
    // without this correction a "circle" computed with equal x/y radius
    // renders as a tall oval.
    let aspect = 2.0;
    let radius_x = radius_y * aspect;
    let width = (radius_x * 2.0).round() as i32 + 1;
    let cx = width / 2;
    let cy = h / 2;

    let mut canvas = Canvas::new(width, h);

    let ellipse_dx = |dy: f64| -> Option<f64> {
        let t = dy / radius_y;
        if t.abs() > 1.0 {
            None
        } else {
            Some(radius_x * (1.0 - t * t).sqrt())
        }
    };

    // Ring.
    for row in 0..h {
        let dy = row as f64 - cy as f64;
        if let Some(dx) = ellipse_dx(dy) {
            canvas.set(row, cx - dx.round() as i32, '●');
            canvas.set(row, cx + dx.round() as i32, '●');
        }
    }

    // 4 corner nodes, one per quadrant, matching the real logo's dots sitting
    // just inside the ring near its top/bottom.
    for &dy_frac in &[-0.78_f64, 0.78] {
        let dy = radius_y * dy_frac;
        if let Some(dx) = ellipse_dx(dy) {
            let row = (cy as f64 + dy).round() as i32;
            canvas.set(row, cx - dx.round() as i32, '◆');
            canvas.set(row, cx + dx.round() as i32, '◆');
        }
    }

    // Interior canal lattice: a handful of thin diagonals crossing behind
    // the V, each with a node dot near its midpoint — evokes the real
    // logo's secondary canals without claiming to reproduce their exact
    // (hand-varied) paths.
    let draw_diagonal = |canvas: &mut Canvas, from: (i32, i32), to: (i32, i32), node_at: f64| {
        let steps = (to.0 - from.0).abs().max((to.1 - from.1).abs()).max(1);
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let row = from.0 + ((to.0 - from.0) as f64 * t).round() as i32;
            let col = from.1 + ((to.1 - from.1) as f64 * t).round() as i32;
            canvas.set(row, col, if row % 2 == 0 { '─' } else { '╲' });
            if (t - node_at).abs() < 1.0 / steps as f64 {
                canvas.set(row, col, '○');
            }
        }
    };
    let r = radius_x.min(radius_y * aspect) * 0.85;
    draw_diagonal(
        &mut canvas,
        (cy - (radius_y * 0.5) as i32, cx - r as i32),
        (cy, cx),
        0.5,
    );
    draw_diagonal(
        &mut canvas,
        (cy - (radius_y * 0.5) as i32, cx + r as i32),
        (cy, cx),
        0.5,
    );
    draw_diagonal(
        &mut canvas,
        (cy + (radius_y * 0.6) as i32, cx - r as i32),
        (cy + (radius_y * 0.2) as i32, cx - (r * 0.3) as i32),
        0.5,
    );
    draw_diagonal(
        &mut canvas,
        (cy + (radius_y * 0.6) as i32, cx + r as i32),
        (cy + (radius_y * 0.2) as i32, cx + (r * 0.3) as i32),
        0.5,
    );

    // The bold V, painted last so it sits in front of the lattice — sized
    // to span most of the circle's interior, same converging-stroke
    // arithmetic as before rather than a hand-typed shape.
    let v_height = (radius_y * 1.5).round() as i32;
    let v_top = cy - v_height + (radius_y * 0.35) as i32;
    let v_glyph_width = 2 * (v_height - 1) + 3;
    for vr in 0..v_height {
        let left = vr;
        let right = v_glyph_width - 1 - vr;
        for w in 0..2 {
            canvas.set(v_top + vr, cx - v_glyph_width / 2 + left + w, '█');
            canvas.set(v_top + vr, cx - v_glyph_width / 2 + right - w, '█');
        }
    }
    // Small tail continuing below the V's point down to the ring, with a
    // node where it meets the bottom — matches the real logo's point not
    // stopping abruptly at the V's apex.
    let tail_start = v_top + v_height - 1;
    for i in 0..3 {
        canvas.set(tail_start + i, cx, '█');
    }
    canvas.set(tail_start + 3, cx, '○');

    canvas.into_lines()
}

fn splash_caption_lines() -> Vec<Line<'static>> {
    vec![
        Line::default(),
        Line::from(Span::styled(
            "V E N I C E",
            Style::default()
                .fg(VENICE_TEAL)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(Span::styled(
            "kernel d'observabilité agentique",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ))
        .alignment(Alignment::Center),
        Line::default(),
        Line::from(Span::styled(
            "appuyez sur une touche pour continuer...",
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
/// (`venice_badge_lines`) rather than showing a blank gap where the real
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
            let mut lines = venice_badge_lines(badge_height);
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
    }

    draw_footer(frame, chunks[2], app);
}

/// Shared border style so every panel reads as one app, not a grab-bag of
/// default-white ratatui boxes.
fn venice_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(VENICE_TEAL))
        .title(Span::styled(
            title,
            Style::default()
                .fg(VENICE_TEAL)
                .add_modifier(Modifier::BOLD),
        ))
}

fn draw_tabs(frame: &mut Frame, area: Rect, current: View) {
    let titles = ["Traces", "Détail", "Métriques"];
    let selected = match current {
        View::Traces => 0,
        View::TraceDetail => 1,
        View::Metrics => 2,
    };
    let tabs = Tabs::new(titles.to_vec())
        .block(venice_block(" Venice "))
        .select(selected)
        .highlight_style(
            Style::default()
                .fg(VENICE_TEAL)
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
        .block(venice_block(
            " Traces récentes (↑/↓, Entrée pour le détail, r pour rafraîchir) ",
        ))
        .highlight_style(
            Style::default()
                .bg(VENICE_TEAL)
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
        " Trace {} — {} spans (Échap pour revenir) ",
        app.trace_spans
            .first()
            .map(|s| s.trace_id.as_str())
            .unwrap_or(""),
        app.trace_spans.len()
    );

    let paragraph = Paragraph::new(lines).block(venice_block(&title));
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
                            .fg(VENICE_TEAL)
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

    let paragraph = Paragraph::new(lines).block(venice_block(" Résumé (r pour rafraîchir) "));
    frame.render_widget(paragraph, area);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let text = app
        .status
        .clone()
        .unwrap_or_else(|| "Tab: changer de vue · q: quitter".to_string());
    frame.render_widget(Paragraph::new(text), area);
}

//! App state and pure logic (span tree reconstruction, timestamp
//! formatting) — kept separate from `ui.rs`/`main.rs` so it's testable
//! without a terminal.

use query_api::dto::{MetricsSummaryDto, SpanDto, TraceSummaryDto};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Traces,
    TraceDetail,
    Metrics,
    Help,
}

pub struct App {
    pub view: View,
    pub traces: Vec<TraceSummaryDto>,
    pub selected_trace: usize,
    pub trace_spans: Vec<SpanDto>,
    pub metrics: Option<MetricsSummaryDto>,
    pub status: Option<String>,
    pub should_quit: bool,
    /// Which page of `content::pages()` the "Help" tab currently shows —
    /// reachable any time from the main app, not just at startup, same
    /// content the paginated intro uses (`content.rs`, one source of
    /// truth for both).
    pub help_page: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            view: View::Traces,
            traces: Vec::new(),
            selected_trace: 0,
            trace_spans: Vec::new(),
            metrics: None,
            status: Some("chargement...".to_string()),
            should_quit: false,
            help_page: 0,
        }
    }

    pub fn select_next(&mut self) {
        if !self.traces.is_empty() {
            self.selected_trace = (self.selected_trace + 1).min(self.traces.len() - 1);
        }
    }

    pub fn select_prev(&mut self) {
        self.selected_trace = self.selected_trace.saturating_sub(1);
    }

    pub fn help_next_page(&mut self, page_count: usize) {
        if page_count > 0 {
            self.help_page = (self.help_page + 1).min(page_count - 1);
        }
    }

    pub fn help_prev_page(&mut self) {
        self.help_page = self.help_page.saturating_sub(1);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// Reconstructs a trace's span tree client-side, exactly as
/// `docs/interfaces/query-api.md` says a client should — `GET
/// /traces/{trace_id}` returns a flat list, not a nested structure, so this
/// is that reconstruction. Guards against cycles/multiple roots on
/// malformed data (query-api doesn't validate a single-root tree server-side
/// either) by tracking visited span ids rather than trusting the shape.
pub fn span_tree(spans: &[SpanDto]) -> Vec<(usize, &SpanDto)> {
    use std::collections::{HashMap, HashSet};

    let by_id: HashMap<&str, &SpanDto> = spans.iter().map(|s| (s.span_id.as_str(), s)).collect();
    let mut children: HashMap<&str, Vec<&SpanDto>> = HashMap::new();
    let mut roots: Vec<&SpanDto> = Vec::new();

    for span in spans {
        match span.parent_span_id.as_deref() {
            Some(parent_id) if by_id.contains_key(parent_id) => {
                children.entry(parent_id).or_default().push(span);
            }
            // No parent, or a parent id that isn't in this trace's span
            // set (shouldn't happen, but not assumed) — treat as a root.
            _ => roots.push(span),
        }
    }

    fn visit<'a>(
        span: &'a SpanDto,
        depth: usize,
        children: &HashMap<&str, Vec<&'a SpanDto>>,
        visited: &mut HashSet<&'a str>,
        result: &mut Vec<(usize, &'a SpanDto)>,
    ) {
        if !visited.insert(span.span_id.as_str()) {
            return;
        }
        result.push((depth, span));
        if let Some(kids) = children.get(span.span_id.as_str()) {
            for kid in kids {
                visit(kid, depth + 1, children, visited, result);
            }
        }
    }

    let mut result = Vec::with_capacity(spans.len());
    let mut visited = HashSet::new();
    for root in &roots {
        visit(root, 0, &children, &mut visited, &mut result);
    }
    // Anything left over is part of a cycle with no reachable root — still
    // shown, not silently dropped, just not indented under anything.
    for span in spans {
        if !visited.contains(span.span_id.as_str()) {
            visit(span, 0, &children, &mut visited, &mut result);
        }
    }

    result
}

/// Coarse "how long ago" for a `start_time_unix_nano` — chosen over an
/// absolute timestamp to avoid a new date/time dependency for what an
/// observability TUI mostly needs anyway: recency, not a calendar date.
pub fn humanize_ago(start_time_unix_nano: u64, now_unix_nano: u64) -> String {
    let elapsed_secs = now_unix_nano.saturating_sub(start_time_unix_nano) / 1_000_000_000;
    match elapsed_secs {
        0..=59 => format!("{elapsed_secs}s"),
        60..=3599 => format!("{}m", elapsed_secs / 60),
        3600..=86399 => format!("{}h", elapsed_secs / 3600),
        _ => format!("{}j", elapsed_secs / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_next_page_stops_at_the_last_page() {
        let mut app = App::new();
        for _ in 0..10 {
            app.help_next_page(3);
        }
        assert_eq!(app.help_page, 2);
    }

    #[test]
    fn help_prev_page_stops_at_zero() {
        let mut app = App::new();
        app.help_prev_page();
        assert_eq!(app.help_page, 0);
    }

    #[test]
    fn help_next_page_is_a_no_op_on_an_empty_page_list() {
        let mut app = App::new();
        app.help_next_page(0);
        assert_eq!(app.help_page, 0);
    }

    fn span(id: &str, parent: Option<&str>) -> SpanDto {
        SpanDto {
            trace_id: "t".to_string(),
            span_id: id.to_string(),
            parent_span_id: parent.map(str::to_string),
            kind: "agent_run".to_string(),
            start_time_unix_nano: 0,
            end_time_unix_nano: 0,
            status_code: "ok".to_string(),
            status_message: String::new(),
            error_type: None,
            operation_name: "invoke_agent".to_string(),
            provider_name: None,
            request_model: None,
            response_model: None,
            input_tokens: None,
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            finish_reasons: vec![],
            conversation_id: None,
            cost_usd: None,
            tool_name: None,
            tool_call_id: None,
            tool_type: None,
            tool_description: None,
            agent_invocation_kind: None,
            agent_name: None,
            agent_id: None,
            agent_description: None,
            agent_version: None,
            extra_attributes: Default::default(),
        }
    }

    #[test]
    fn builds_depth_from_a_simple_parent_child_chain() {
        let spans = vec![span("a", None), span("b", Some("a")), span("c", Some("b"))];
        let tree = span_tree(&spans);
        let depths: Vec<(usize, &str)> =
            tree.iter().map(|(d, s)| (*d, s.span_id.as_str())).collect();
        assert_eq!(depths, vec![(0, "a"), (1, "b"), (2, "c")]);
    }

    #[test]
    fn handles_multiple_roots() {
        let spans = vec![span("a", None), span("b", None)];
        let tree = span_tree(&spans);
        assert_eq!(tree.len(), 2);
        assert!(tree.iter().all(|(d, _)| *d == 0));
    }

    #[test]
    fn a_parent_id_pointing_outside_the_trace_is_treated_as_a_root_not_dropped() {
        let spans = vec![span("a", Some("does-not-exist"))];
        let tree = span_tree(&spans);
        assert_eq!(tree, vec![(0, &spans[0])]);
    }

    #[test]
    fn a_two_node_cycle_does_not_infinite_loop_and_nothing_is_lost() {
        let spans = vec![span("a", Some("b")), span("b", Some("a"))];
        let tree = span_tree(&spans);
        // No root exists (both have a parent), so both are recovered by the
        // leftover pass — the point is this terminates and keeps both spans.
        assert_eq!(tree.len(), 2);
    }

    #[test]
    fn humanize_ago_picks_the_coarsest_useful_unit() {
        let now = 100 * 1_000_000_000;
        assert_eq!(humanize_ago(now - 5 * 1_000_000_000, now), "5s");
        assert_eq!(humanize_ago(now - 90 * 1_000_000_000, now), "1m");
    }
}

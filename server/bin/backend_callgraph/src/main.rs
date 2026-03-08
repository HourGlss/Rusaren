//! Generates a filtered backend-only call graph from a binary `index.scip` file.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used, clippy::panic_in_result_fn))]

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use protobuf::Message;
use scip::types::{Document, Index, Occurrence};
use serde::Serialize;

const KIND_FUNCTION: i32 = 17;
const KIND_METHOD: i32 = 26;
const KIND_STATIC_METHOD: i32 = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    input_path: PathBuf,
    output_dir: PathBuf,
    backend_files: BTreeSet<String>,
    entry_files: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FunctionSpan {
    start_line: usize,
    body_start_line: usize,
    end_line: usize,
    body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Node {
    symbol: String,
    full_name: String,
    short_label: String,
    file_relative_path: String,
    start_line: usize,
    body_start_line: usize,
    end_line: usize,
    outgoing: BTreeSet<String>,
    incoming: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct RankedNode {
    label: String,
    file: String,
    count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ExternalReference {
    crate_name: String,
    count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct RankedFile {
    file: String,
    function_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct RankedFileEdge {
    source_file: String,
    target_file: String,
    count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct GraphSummary {
    node_count: usize,
    edge_count: usize,
    file_count: usize,
    overview_file_count: usize,
    overview_edge_count: usize,
    root_count: usize,
    omitted_test_nodes: usize,
    omitted_bodyless_nodes: usize,
    omitted_unreachable_nodes: usize,
    roots: Vec<String>,
    files: Vec<String>,
    file_function_counts: Vec<RankedFile>,
    top_file_edges: Vec<RankedFileEdge>,
    top_fan_out: Vec<RankedNode>,
    top_fan_in: Vec<RankedNode>,
    hidden_external_references: Vec<ExternalReference>,
}

#[derive(Debug, Clone)]
struct BuildResult {
    nodes: BTreeMap<String, Node>,
    roots: Vec<String>,
    omitted_test_nodes: usize,
    omitted_bodyless_nodes: usize,
    omitted_unreachable_nodes: usize,
    hidden_external_references: Vec<ExternalReference>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OverviewNode {
    file_relative_path: String,
    function_count: usize,
    outgoing: BTreeMap<String, usize>,
    incoming: BTreeMap<String, usize>,
    is_root: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OverviewGraph {
    nodes: BTreeMap<String, OverviewNode>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    if args.len() < 3 {
        return Err(format!(
            "usage: {} <input-index.scip> <output-dir> --backend-file <path>... --entry-file <path>...",
            args.first().map_or("backend_callgraph", String::as_str)
        ));
    }

    let mut backend_files = BTreeSet::new();
    let mut entry_files = BTreeSet::new();
    let mut index = 3;
    while index < args.len() {
        let flag = args
            .get(index)
            .ok_or_else(|| String::from("missing flag while parsing arguments"))?;
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("flag {flag} requires a path value"))?;

        match flag.as_str() {
            "--backend-file" => {
                backend_files.insert(normalize_relative_path(value));
            }
            "--entry-file" => {
                entry_files.insert(normalize_relative_path(value));
            }
            _ => return Err(format!("unsupported argument: {flag}")),
        }

        index += 2;
    }

    if backend_files.is_empty() {
        return Err(String::from(
            "at least one --backend-file path is required to build a backend graph",
        ));
    }

    if entry_files.is_empty() {
        return Err(String::from(
            "at least one --entry-file path is required to select graph entrypoints",
        ));
    }

    Ok(Args {
        input_path: PathBuf::from(&args[1]),
        output_dir: PathBuf::from(&args[2]),
        backend_files,
        entry_files,
    })
}

fn normalize_relative_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn parse_project_root(raw: &str) -> PathBuf {
    let trimmed = raw.trim_start_matches("file://");
    #[cfg(windows)]
    let trimmed = if trimmed.starts_with('/') && trimmed.as_bytes().get(2) == Some(&b':') {
        &trimmed[1..]
    } else {
        trimmed
    };

    PathBuf::from(trimmed)
}

fn is_callable_kind(kind: i32) -> bool {
    matches!(kind, KIND_FUNCTION | KIND_METHOD | KIND_STATIC_METHOD)
}

fn normalize_symbol_name(symbol: &str) -> String {
    let parts: Vec<&str> = symbol.split(' ').collect();
    if parts.len() < 5 {
        return symbol.to_string();
    }

    let crate_name = parts[2];
    let path_part = parts[4..].join(" ");
    let cleaned = path_part
        .trim_end_matches('.')
        .trim_end_matches("()")
        .replace('#', "::");

    let components = cleaned
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();

    if components.is_empty() {
        crate_name.to_string()
    } else {
        format!("{crate_name}::{}", components.join("::"))
    }
}

fn short_label(full_name: &str) -> String {
    let parts: Vec<&str> = full_name.split("::").collect();
    if parts.len() >= 3 {
        parts[parts.len() - 3..].join("::")
    } else {
        full_name.to_string()
    }
}

fn source_file_path(project_root: &Path, relative_path: &str) -> PathBuf {
    let mut path = project_root.to_path_buf();
    for component in relative_path.split('/') {
        path.push(component);
    }
    path
}

fn line_number(occurrence: &Occurrence) -> Option<usize> {
    occurrence
        .range
        .first()
        .and_then(|line| usize::try_from(*line).ok())
}

fn is_definition_occurrence(occurrence: &Occurrence) -> bool {
    occurrence.symbol_roles & 1 == 1
}

fn find_definition_line(document: &Document, symbol: &str) -> Option<usize> {
    document
        .occurrences
        .iter()
        .filter(|occurrence| is_definition_occurrence(occurrence) && occurrence.symbol == symbol)
        .filter_map(line_number)
        .min()
}

fn extract_function_span(lines: &[String], start_line: usize) -> Option<FunctionSpan> {
    if start_line >= lines.len() {
        return None;
    }

    let mut body_lines = Vec::new();
    let mut body_start_line = None;
    let mut brace_depth = 0_usize;
    let mut opened = false;

    for (line_index, line) in lines.iter().enumerate().skip(start_line) {
        body_lines.push(line.clone());
        for ch in line.chars() {
            if ch == '{' {
                opened = true;
                if body_start_line.is_none() {
                    body_start_line = Some(line_index);
                }
                brace_depth = brace_depth.saturating_add(1);
            } else if ch == '}' && opened {
                brace_depth = brace_depth.saturating_sub(1);
                if brace_depth == 0 {
                    return Some(FunctionSpan {
                        start_line,
                        body_start_line: body_start_line.unwrap_or(start_line),
                        end_line: line_index,
                        body: body_lines.join("\n"),
                    });
                }
            }
        }
    }

    None
}

fn is_test_function(full_name: &str, lines: &[String], start_line: usize) -> bool {
    if full_name.contains("::tests::") {
        return true;
    }

    let lookback = start_line.saturating_sub(3);
    lines[lookback..=start_line]
        .iter()
        .map(|line| line.trim())
        .any(|line| {
            (line.starts_with("#[") || line.starts_with("# !["))
                && (line.contains("test") || line.contains("rstest"))
        })
}

fn looks_like_external_callable(symbol: &str) -> bool {
    symbol.contains("().") && (symbol.contains('#') || symbol.contains('/')) && !symbol.contains("().(")
}

fn external_crate_name(symbol: &str) -> Option<String> {
    let parts: Vec<&str> = symbol.split(' ').collect();
    parts.get(2).map(|part| (*part).to_string())
}

fn xml_escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn dot_escape(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('"', "\\\"")
}

fn find_enclosing_symbol(
    symbols_for_file: &[(String, usize, usize)],
    line: usize,
) -> Option<String> {
    symbols_for_file
        .iter()
        .filter(|(_, body_start, end_line)| line >= *body_start && line <= *end_line)
        .min_by_key(|(_, body_start, end_line)| end_line.saturating_sub(*body_start))
        .map(|(symbol, _, _)| symbol.clone())
}

fn select_roots(nodes: &BTreeMap<String, Node>, entry_files: &BTreeSet<String>) -> Vec<String> {
    let entry_candidates = nodes
        .values()
        .filter(|node| entry_files.contains(&node.file_relative_path))
        .collect::<Vec<_>>();
    let preferred_entry_candidates = entry_candidates
        .iter()
        .copied()
        .filter(|node| !node.full_name.contains("::impl::"))
        .collect::<Vec<_>>();

    let mut roots = nodes
        .values()
        .filter(|node| {
            entry_files.contains(&node.file_relative_path)
                && node.incoming.is_empty()
                && !node.full_name.contains("::impl::")
        })
        .map(|node| node.symbol.clone())
        .collect::<Vec<_>>();

    if roots.is_empty() {
        roots = preferred_entry_candidates
            .iter()
            .map(|node| node.symbol.clone())
            .collect::<Vec<_>>();
    }

    if roots.is_empty() {
        roots = entry_candidates
            .iter()
            .map(|node| node.symbol.clone())
            .collect::<Vec<_>>();
    }

    if roots.is_empty() {
        roots = nodes
            .values()
            .filter(|node| node.incoming.is_empty())
            .map(|node| node.symbol.clone())
            .collect::<Vec<_>>();
    }

    if roots.is_empty() {
        roots = nodes.keys().take(1).cloned().collect();
    }

    roots.sort();
    roots.dedup();
    roots
}

fn reachable_from_roots(nodes: &BTreeMap<String, Node>, roots: &[String]) -> BTreeSet<String> {
    let mut reachable = BTreeSet::new();
    let mut queue = VecDeque::from(roots.to_vec());

    while let Some(symbol) = queue.pop_front() {
        if !reachable.insert(symbol.clone()) {
            continue;
        }

        if let Some(node) = nodes.get(&symbol) {
            for callee in &node.outgoing {
                if nodes.contains_key(callee) {
                    queue.push_back(callee.clone());
                }
            }
        }
    }

    reachable
}

#[allow(clippy::too_many_lines)]
fn build_graph(index: &Index, args: &Args) -> Result<BuildResult, String> {
    let project_root = parse_project_root(index.metadata.as_ref().map_or("", |metadata| {
        metadata.project_root.as_str()
    }));

    let mut source_lines_by_file = BTreeMap::new();
    let mut document_by_file = BTreeMap::new();

    for document in &index.documents {
        let relative_path = normalize_relative_path(&document.relative_path);
        if !args.backend_files.contains(&relative_path) {
            continue;
        }

        let source_path = source_file_path(&project_root, &relative_path);
        let source = fs::read_to_string(&source_path)
            .map_err(|error| format!("failed to read {}: {error}", source_path.display()))?;
        let lines = source.lines().map(str::to_string).collect::<Vec<_>>();

        source_lines_by_file.insert(relative_path.clone(), lines);
        document_by_file.insert(relative_path, document.clone());
    }

    let mut nodes = BTreeMap::new();
    let mut omitted_test_nodes = 0_usize;
    let mut omitted_bodyless_nodes = 0_usize;

    for (relative_path, document) in &document_by_file {
        let Some(lines) = source_lines_by_file.get(relative_path) else {
            continue;
        };

        for symbol in &document.symbols {
            let kind = symbol.kind.value();
            if !is_callable_kind(kind) {
                continue;
            }

            let Some(start_line) = find_definition_line(document, &symbol.symbol) else {
                omitted_bodyless_nodes = omitted_bodyless_nodes.saturating_add(1);
                continue;
            };

            let full_name = normalize_symbol_name(&symbol.symbol);
            if is_test_function(&full_name, lines, start_line) {
                omitted_test_nodes = omitted_test_nodes.saturating_add(1);
                continue;
            }

            let Some(span) = extract_function_span(lines, start_line) else {
                omitted_bodyless_nodes = omitted_bodyless_nodes.saturating_add(1);
                continue;
            };

            nodes.insert(
                symbol.symbol.clone(),
                Node {
                    symbol: symbol.symbol.clone(),
                    full_name: full_name.clone(),
                    short_label: short_label(&full_name),
                    file_relative_path: relative_path.clone(),
                    start_line: span.start_line,
                    body_start_line: span.body_start_line,
                    end_line: span.end_line,
                    outgoing: BTreeSet::new(),
                    incoming: BTreeSet::new(),
                },
            );
        }
    }

    let symbol_to_file_ranges = document_by_file
        .keys()
        .map(|relative_path| {
            let mut ranges = nodes
                .values()
                .filter(|node| node.file_relative_path == *relative_path)
                .map(|node| (node.symbol.clone(), node.body_start_line, node.end_line))
                .collect::<Vec<_>>();
            ranges.sort_by_key(|(_, body_start, end_line)| (*body_start, *end_line));
            (relative_path.clone(), ranges)
        })
        .collect::<BTreeMap<_, _>>();

    let mut caller_external_refs = HashMap::<String, HashMap<String, usize>>::new();

    for (relative_path, document) in &document_by_file {
        let Some(ranges) = symbol_to_file_ranges.get(relative_path) else {
            continue;
        };

        for occurrence in &document.occurrences {
            if is_definition_occurrence(occurrence) {
                continue;
            }

            let Some(line) = line_number(occurrence) else {
                continue;
            };

            let Some(caller_symbol) = find_enclosing_symbol(ranges, line) else {
                continue;
            };

            if nodes.contains_key(&occurrence.symbol) {
                if caller_symbol == occurrence.symbol {
                    continue;
                }

                if let Some(caller) = nodes.get_mut(&caller_symbol) {
                    caller.outgoing.insert(occurrence.symbol.clone());
                }
                if let Some(callee) = nodes.get_mut(&occurrence.symbol) {
                    callee.incoming.insert(caller_symbol.clone());
                }
            } else if looks_like_external_callable(&occurrence.symbol) {
                let crate_name = external_crate_name(&occurrence.symbol)
                    .unwrap_or_else(|| String::from("unknown"));
                let counts = caller_external_refs.entry(caller_symbol).or_default();
                let counter = counts.entry(crate_name).or_insert(0);
                *counter = counter.saturating_add(1);
            }
        }
    }

    let roots = select_roots(&nodes, &args.entry_files);
    let reachable = reachable_from_roots(&nodes, &roots);
    let omitted_unreachable_nodes = nodes.len().saturating_sub(reachable.len());

    nodes.retain(|symbol, _| reachable.contains(symbol));
    for node in nodes.values_mut() {
        node.outgoing.retain(|callee| reachable.contains(callee));
        node.incoming.retain(|caller| reachable.contains(caller));
    }

    let mut external_counts = HashMap::<String, usize>::new();
    for (caller_symbol, references) in caller_external_refs {
        if !nodes.contains_key(&caller_symbol) {
            continue;
        }

        for (crate_name, count) in references {
            let counter = external_counts.entry(crate_name).or_insert(0);
            *counter = counter.saturating_add(count);
        }
    }

    let mut hidden_external_references = external_counts
        .into_iter()
        .map(|(crate_name, count)| ExternalReference { crate_name, count })
        .collect::<Vec<_>>();
    hidden_external_references.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.crate_name.cmp(&right.crate_name))
    });
    hidden_external_references.truncate(10);

    Ok(BuildResult {
        nodes,
        roots,
        omitted_test_nodes,
        omitted_bodyless_nodes,
        omitted_unreachable_nodes,
        hidden_external_references,
    })
}

#[allow(clippy::too_many_lines)]
fn build_summary(result: &BuildResult) -> GraphSummary {
    let node_count = result.nodes.len();
    let edge_count = result.nodes.values().map(|node| node.outgoing.len()).sum();
    let overview = build_overview_graph(result);

    let mut files = result
        .nodes
        .values()
        .map(|node| node.file_relative_path.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    files.sort();

    let mut top_fan_out = result
        .nodes
        .values()
        .map(|node| RankedNode {
            label: node.full_name.clone(),
            file: node.file_relative_path.clone(),
            count: node.outgoing.len(),
        })
        .collect::<Vec<_>>();
    top_fan_out.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.label.cmp(&right.label))
    });
    top_fan_out.truncate(10);

    let mut top_fan_in = result
        .nodes
        .values()
        .map(|node| RankedNode {
            label: node.full_name.clone(),
            file: node.file_relative_path.clone(),
            count: node.incoming.len(),
        })
        .collect::<Vec<_>>();
    top_fan_in.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.label.cmp(&right.label))
    });
    top_fan_in.truncate(10);

    let mut file_function_counts = overview
        .nodes
        .values()
        .map(|node| RankedFile {
            file: node.file_relative_path.clone(),
            function_count: node.function_count,
        })
        .collect::<Vec<_>>();
    file_function_counts.sort_by(|left, right| {
        right
            .function_count
            .cmp(&left.function_count)
            .then_with(|| left.file.cmp(&right.file))
    });

    let mut top_file_edges = overview
        .nodes
        .values()
        .flat_map(|node| {
            node.outgoing
                .iter()
                .map(move |(target_file, count)| RankedFileEdge {
                    source_file: node.file_relative_path.clone(),
                    target_file: target_file.clone(),
                    count: *count,
                })
        })
        .collect::<Vec<_>>();
    top_file_edges.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.source_file.cmp(&right.source_file))
            .then_with(|| left.target_file.cmp(&right.target_file))
    });
    let overview_edge_count = top_file_edges.len();
    top_file_edges.truncate(10);

    GraphSummary {
        node_count,
        edge_count,
        file_count: files.len(),
        overview_file_count: overview.nodes.len(),
        overview_edge_count,
        root_count: result.roots.len(),
        omitted_test_nodes: result.omitted_test_nodes,
        omitted_bodyless_nodes: result.omitted_bodyless_nodes,
        omitted_unreachable_nodes: result.omitted_unreachable_nodes,
        roots: result
            .roots
            .iter()
            .filter_map(|symbol| result.nodes.get(symbol))
            .map(|node| node.full_name.clone())
            .collect(),
        files,
        file_function_counts,
        top_file_edges,
        top_fan_out,
        top_fan_in,
        hidden_external_references: result.hidden_external_references.clone(),
    }
}

fn build_overview_graph(result: &BuildResult) -> OverviewGraph {
    let root_symbols = result.roots.iter().cloned().collect::<BTreeSet<_>>();
    let mut nodes = BTreeMap::<String, OverviewNode>::new();

    for node in result.nodes.values() {
        let entry = nodes
            .entry(node.file_relative_path.clone())
            .or_insert_with(|| OverviewNode {
                file_relative_path: node.file_relative_path.clone(),
                function_count: 0,
                outgoing: BTreeMap::new(),
                incoming: BTreeMap::new(),
                is_root: false,
            });
        entry.function_count = entry.function_count.saturating_add(1);
        if root_symbols.contains(&node.symbol) {
            entry.is_root = true;
        }
    }

    for node in result.nodes.values() {
        for callee_symbol in &node.outgoing {
            let Some(callee) = result.nodes.get(callee_symbol) else {
                continue;
            };

            if node.file_relative_path == callee.file_relative_path {
                continue;
            }

            let source = nodes
                .entry(node.file_relative_path.clone())
                .or_insert_with(|| OverviewNode {
                    file_relative_path: node.file_relative_path.clone(),
                    function_count: 0,
                    outgoing: BTreeMap::new(),
                    incoming: BTreeMap::new(),
                    is_root: false,
                });
            let edge_count = source
                .outgoing
                .entry(callee.file_relative_path.clone())
                .or_insert(0);
            *edge_count = edge_count.saturating_add(1);

            let target = nodes
                .entry(callee.file_relative_path.clone())
                .or_insert_with(|| OverviewNode {
                    file_relative_path: callee.file_relative_path.clone(),
                    function_count: 0,
                    outgoing: BTreeMap::new(),
                    incoming: BTreeMap::new(),
                    is_root: false,
                });
            let incoming_count = target
                .incoming
                .entry(node.file_relative_path.clone())
                .or_insert(0);
            *incoming_count = incoming_count.saturating_add(1);
        }
    }

    OverviewGraph { nodes }
}

fn node_fill(file_relative_path: &str, roots: &BTreeSet<String>, symbol: &str) -> &'static str {
    if roots.contains(symbol) {
        return "#fde68a";
    }

    if file_relative_path.contains("game_api") {
        return "#dbeafe";
    }
    if file_relative_path.contains("game_net") {
        return "#dcfce7";
    }
    if file_relative_path.contains("game_match") || file_relative_path.contains("game_lobby") {
        return "#fee2e2";
    }
    if file_relative_path.contains("game_sim") {
        return "#ede9fe";
    }

    "#e5e7eb"
}

fn file_fill(file_relative_path: &str, is_root: bool) -> &'static str {
    if is_root {
        return "#fde68a";
    }

    if file_relative_path.contains("game_api") {
        return "#dbeafe";
    }
    if file_relative_path.contains("game_net") {
        return "#dcfce7";
    }
    if file_relative_path.contains("game_match") || file_relative_path.contains("game_lobby") {
        return "#fee2e2";
    }
    if file_relative_path.contains("game_sim") {
        return "#ede9fe";
    }

    "#e5e7eb"
}

fn compute_levels(nodes: &BTreeMap<String, Node>, roots: &[String]) -> BTreeMap<String, usize> {
    let mut levels: BTreeMap<String, usize> = BTreeMap::new();
    let mut queue = VecDeque::new();

    for root in roots {
        levels.insert(root.clone(), 0);
        queue.push_back(root.clone());
    }

    while let Some(symbol) = queue.pop_front() {
        let Some(current_level) = levels.get(&symbol).copied() else {
            continue;
        };

        if let Some(node) = nodes.get(&symbol) {
            for callee in &node.outgoing {
                if !nodes.contains_key(callee) || levels.contains_key(callee) {
                    continue;
                }

                levels.insert(callee.clone(), current_level.saturating_add(1));
                queue.push_back(callee.clone());
            }
        }
    }

    for symbol in nodes.keys() {
        levels.entry(symbol.clone()).or_insert(0);
    }

    levels
}

fn compute_overview_levels(nodes: &BTreeMap<String, OverviewNode>) -> BTreeMap<String, usize> {
    let mut levels: BTreeMap<String, usize> = BTreeMap::new();
    let mut queue = VecDeque::new();
    let mut roots = nodes
        .values()
        .filter(|node| node.is_root)
        .map(|node| node.file_relative_path.clone())
        .collect::<Vec<_>>();

    if roots.is_empty() {
        roots = nodes
            .values()
            .filter(|node| node.incoming.is_empty())
            .map(|node| node.file_relative_path.clone())
            .collect();
    }

    if roots.is_empty() {
        roots = nodes.keys().take(1).cloned().collect();
    }

    for root in roots {
        levels.insert(root.clone(), 0);
        queue.push_back(root);
    }

    while let Some(file) = queue.pop_front() {
        let Some(current_level) = levels.get(&file).copied() else {
            continue;
        };

        if let Some(node) = nodes.get(&file) {
            for target_file in node.outgoing.keys() {
                if levels.contains_key(target_file) {
                    continue;
                }

                levels.insert(target_file.clone(), current_level.saturating_add(1));
                queue.push_back(target_file.clone());
            }
        }
    }

    for file in nodes.keys() {
        levels.entry(file.clone()).or_insert(0);
    }

    levels
}

#[allow(clippy::format_push_string)]
fn write_dot(path: &Path, result: &BuildResult) -> Result<(), Box<dyn Error>> {
    let mut node_ids = BTreeMap::new();
    for (index, symbol) in result.nodes.keys().enumerate() {
        node_ids.insert(symbol.clone(), format!("n{index}"));
    }

    let roots = result.roots.iter().cloned().collect::<BTreeSet<_>>();
    let mut dot = String::from(
        "digraph backend_core {\n  rankdir=LR;\n  graph [splines=true, overlap=false, pad=0.25, nodesep=0.35, ranksep=0.55];\n  node [shape=box, style=\"rounded,filled\", fontname=\"Segoe UI\", fontsize=10, color=\"#94a3b8\"];\n  edge [color=\"#64748b\", arrowsize=0.7];\n",
    );

    let mut files = result
        .nodes
        .values()
        .map(|node| node.file_relative_path.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    files.sort();

    for file in files {
        let cluster_id = file.replace(['/', '.'], "_");
        dot.push_str(&format!(
            "  subgraph cluster_{cluster_id} {{\n    label=\"{}\";\n    color=\"#cbd5e1\";\n",
            dot_escape(&file)
        ));

        let mut file_nodes = result
            .nodes
            .values()
            .filter(|node| node.file_relative_path == file)
            .collect::<Vec<_>>();
        file_nodes.sort_by(|left, right| left.full_name.cmp(&right.full_name));

        for node in file_nodes {
            let Some(node_id) = node_ids.get(&node.symbol) else {
                continue;
            };

            let label = format!(
                "{}\\n{}:{}",
                node.short_label,
                file,
                node.start_line.saturating_add(1)
            );
            let fill = node_fill(&node.file_relative_path, &roots, &node.symbol);
            dot.push_str(&format!(
                "    {node_id} [label=\"{}\", fillcolor=\"{fill}\"];\n",
                dot_escape(&label)
            ));
        }

        dot.push_str("  }\n");
    }

    for node in result.nodes.values() {
        let Some(source_id) = node_ids.get(&node.symbol) else {
            continue;
        };

        for callee in &node.outgoing {
            let Some(target_id) = node_ids.get(callee) else {
                continue;
            };

            dot.push_str(&format!("  {source_id} -> {target_id};\n"));
        }
    }

    dot.push_str("}\n");
    fs::write(path, dot)?;
    Ok(())
}

#[allow(
    clippy::cast_precision_loss,
    clippy::format_push_string,
    clippy::too_many_lines
)]
fn write_svg(path: &Path, result: &BuildResult) -> Result<(), Box<dyn Error>> {
    let roots = result.roots.iter().cloned().collect::<BTreeSet<_>>();
    if result.nodes.is_empty() {
        fs::write(
            path,
            "<svg xmlns='http://www.w3.org/2000/svg' width='800' height='160'><text x='24' y='40'>No backend call graph nodes were selected.</text></svg>",
        )?;
        return Ok(());
    }

    let levels = compute_levels(&result.nodes, &result.roots);
    let max_level = levels.values().copied().max().unwrap_or(0);
    let mut columns = vec![Vec::<String>::new(); max_level.saturating_add(1)];
    for (symbol, level) in &levels {
        if let Some(column) = columns.get_mut(*level) {
            column.push(symbol.clone());
        }
    }

    for column in &mut columns {
        column.sort_by(|left, right| {
            let left_node = &result.nodes[left];
            let right_node = &result.nodes[right];
            left_node
                .file_relative_path
                .cmp(&right_node.file_relative_path)
                .then_with(|| left_node.full_name.cmp(&right_node.full_name))
        });
    }

    let node_width = 280.0_f32;
    let node_height = 52.0_f32;
    let column_gap = 80.0_f32;
    let row_gap = 18.0_f32;
    let margin_x = 36.0_f32;
    let margin_y = 48.0_f32;
    let width = margin_x * 2.0
        + (max_level as f32 + 1.0) * node_width
        + (max_level as f32) * column_gap;
    let tallest_column = columns.iter().map(Vec::len).max().unwrap_or(1) as f32;
    let height = margin_y * 2.0
        + tallest_column * node_height
        + (tallest_column - 1.0).max(0.0) * row_gap;

    let mut positions = BTreeMap::<String, (f32, f32)>::new();
    for (level, column) in columns.iter().enumerate() {
        for (row, symbol) in column.iter().enumerate() {
            let x = margin_x + level as f32 * (node_width + column_gap);
            let y = margin_y + row as f32 * (node_height + row_gap);
            positions.insert(symbol.clone(), (x, y));
        }
    }

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{width}' height='{height}' viewBox='0 0 {width} {height}' font-family='Segoe UI, Tahoma, sans-serif'>\n"
    ));
    svg.push_str("  <rect width='100%' height='100%' fill='#f8fafc'/>\n");
    svg.push_str("  <defs><marker id='arrow' markerWidth='10' markerHeight='10' refX='9' refY='3' orient='auto' markerUnits='strokeWidth'><path d='M0,0 L10,3 L0,6 z' fill='#94a3b8'/></marker></defs>\n");

    for node in result.nodes.values() {
        let Some((source_x, source_y)) = positions.get(&node.symbol).copied() else {
            continue;
        };

        for callee in &node.outgoing {
            let Some((target_x, target_y)) = positions.get(callee).copied() else {
                continue;
            };

            let x1 = source_x + node_width;
            let y1 = source_y + node_height / 2.0;
            let x2 = target_x;
            let y2 = target_y + node_height / 2.0;
            let cx1 = x1 + column_gap / 2.0;
            let cx2 = x2 - column_gap / 2.0;
            svg.push_str(&format!(
                "  <path d='M {x1:.1} {y1:.1} C {cx1:.1} {y1:.1}, {cx2:.1} {y2:.1}, {x2:.1} {y2:.1}' fill='none' stroke='#94a3b8' stroke-width='1.5' marker-end='url(#arrow)'/>\n"
            ));
        }
    }

    for node in result.nodes.values() {
        let Some((x, y)) = positions.get(&node.symbol).copied() else {
            continue;
        };

        let fill = node_fill(&node.file_relative_path, &roots, &node.symbol);
        let subtitle = format!(
            "{}:{}",
            Path::new(&node.file_relative_path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&node.file_relative_path),
            node.start_line.saturating_add(1)
        );

        svg.push_str(&format!(
            "  <rect x='{x:.1}' y='{y:.1}' width='{node_width:.1}' height='{node_height:.1}' rx='10' ry='10' fill='{fill}' stroke='#475569' stroke-width='1.2'/>\n"
        ));
        svg.push_str(&format!(
            "  <text x='{:.1}' y='{:.1}' font-size='12' font-weight='600' fill='#0f172a'><tspan x='{:.1}' dy='0'>{}</tspan><tspan x='{:.1}' dy='16' font-size='10' font-weight='400' fill='#334155'>{}</tspan></text>\n",
            x + 12.0,
            y + 20.0,
            x + 12.0,
            xml_escape(&node.short_label),
            x + 12.0,
            xml_escape(&subtitle)
        ));
    }

    svg.push_str("</svg>\n");
    fs::write(path, svg)?;
    Ok(())
}

#[allow(clippy::format_push_string)]
fn write_overview_dot(path: &Path, overview: &OverviewGraph) -> Result<(), Box<dyn Error>> {
    let mut node_ids = BTreeMap::new();
    for (index, file) in overview.nodes.keys().enumerate() {
        node_ids.insert(file.clone(), format!("f{index}"));
    }

    let mut dot = String::from(
        "digraph backend_core_overview {\n  rankdir=LR;\n  graph [splines=true, overlap=false, pad=0.25, nodesep=0.5, ranksep=0.75];\n  node [shape=box, style=\"rounded,filled\", fontname=\"Segoe UI\", fontsize=11, color=\"#94a3b8\"];\n  edge [color=\"#64748b\", arrowsize=0.7, fontname=\"Segoe UI\", fontsize=10];\n",
    );

    for node in overview.nodes.values() {
        let Some(node_id) = node_ids.get(&node.file_relative_path) else {
            continue;
        };
        let label = format!(
            "{}\\n{} functions",
            node.file_relative_path, node.function_count
        );
        let fill = file_fill(&node.file_relative_path, node.is_root);
        dot.push_str(&format!(
            "  {node_id} [label=\"{}\", fillcolor=\"{fill}\"];\n",
            dot_escape(&label)
        ));
    }

    for node in overview.nodes.values() {
        let Some(source_id) = node_ids.get(&node.file_relative_path) else {
            continue;
        };

        for (target_file, count) in &node.outgoing {
            let Some(target_id) = node_ids.get(target_file) else {
                continue;
            };
            dot.push_str(&format!(
                "  {source_id} -> {target_id} [label=\"{count}\"];\n"
            ));
        }
    }

    dot.push_str("}\n");
    fs::write(path, dot)?;
    Ok(())
}

#[allow(
    clippy::cast_precision_loss,
    clippy::format_push_string,
    clippy::too_many_lines
)]
fn write_overview_svg(path: &Path, overview: &OverviewGraph) -> Result<(), Box<dyn Error>> {
    if overview.nodes.is_empty() {
        fs::write(
            path,
            "<svg xmlns='http://www.w3.org/2000/svg' width='800' height='160'><text x='24' y='40'>No backend overview nodes were selected.</text></svg>",
        )?;
        return Ok(());
    }

    let levels = compute_overview_levels(&overview.nodes);
    let max_level = levels.values().copied().max().unwrap_or(0);
    let mut columns = vec![Vec::<String>::new(); max_level.saturating_add(1)];
    for (file, level) in &levels {
        if let Some(column) = columns.get_mut(*level) {
            column.push(file.clone());
        }
    }

    for column in &mut columns {
        column.sort();
    }

    let node_width = 320.0_f32;
    let node_height = 64.0_f32;
    let column_gap = 96.0_f32;
    let row_gap = 28.0_f32;
    let margin_x = 36.0_f32;
    let margin_y = 52.0_f32;
    let width = margin_x * 2.0
        + (max_level as f32 + 1.0) * node_width
        + (max_level as f32) * column_gap;
    let tallest_column = columns.iter().map(Vec::len).max().unwrap_or(1) as f32;
    let height = margin_y * 2.0
        + tallest_column * node_height
        + (tallest_column - 1.0).max(0.0) * row_gap;

    let mut positions = BTreeMap::<String, (f32, f32)>::new();
    for (level, column) in columns.iter().enumerate() {
        for (row, file) in column.iter().enumerate() {
            let x = margin_x + level as f32 * (node_width + column_gap);
            let y = margin_y + row as f32 * (node_height + row_gap);
            positions.insert(file.clone(), (x, y));
        }
    }

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{width}' height='{height}' viewBox='0 0 {width} {height}' font-family='Segoe UI, Tahoma, sans-serif'>\n"
    ));
    svg.push_str("  <rect width='100%' height='100%' fill='#f8fafc'/>\n");
    svg.push_str("  <defs><marker id='overview-arrow' markerWidth='10' markerHeight='10' refX='9' refY='3' orient='auto' markerUnits='strokeWidth'><path d='M0,0 L10,3 L0,6 z' fill='#64748b'/></marker></defs>\n");

    for node in overview.nodes.values() {
        let Some((source_x, source_y)) = positions.get(&node.file_relative_path).copied() else {
            continue;
        };

        for (target_file, count) in &node.outgoing {
            let Some((target_x, target_y)) = positions.get(target_file).copied() else {
                continue;
            };

            let x1 = source_x + node_width;
            let y1 = source_y + node_height / 2.0;
            let x2 = target_x;
            let y2 = target_y + node_height / 2.0;
            let cx1 = x1 + column_gap / 2.0;
            let cx2 = x2 - column_gap / 2.0;
            let label_x = f32::midpoint(x1, x2);
            let label_y = f32::midpoint(y1, y2) - 6.0;
            svg.push_str(&format!(
                "  <path d='M {x1:.1} {y1:.1} C {cx1:.1} {y1:.1}, {cx2:.1} {y2:.1}, {x2:.1} {y2:.1}' fill='none' stroke='#64748b' stroke-width='1.8' marker-end='url(#overview-arrow)'/>\n"
            ));
            svg.push_str(&format!(
                "  <text x='{label_x:.1}' y='{label_y:.1}' font-size='11' text-anchor='middle' fill='#334155'>{count}</text>\n"
            ));
        }
    }

    for node in overview.nodes.values() {
        let Some((x, y)) = positions.get(&node.file_relative_path).copied() else {
            continue;
        };
        let fill = file_fill(&node.file_relative_path, node.is_root);
        let file_name = Path::new(&node.file_relative_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&node.file_relative_path);

        svg.push_str(&format!(
            "  <rect x='{x:.1}' y='{y:.1}' width='{node_width:.1}' height='{node_height:.1}' rx='12' ry='12' fill='{fill}' stroke='#475569' stroke-width='1.2'/>\n"
        ));
        svg.push_str(&format!(
            "  <text x='{:.1}' y='{:.1}' font-size='13' font-weight='700' fill='#0f172a'><tspan x='{:.1}' dy='0'>{}</tspan><tspan x='{:.1}' dy='18' font-size='11' font-weight='400' fill='#334155'>{}</tspan><tspan x='{:.1}' dy='16' font-size='11' font-weight='400' fill='#334155'>{} functions</tspan></text>\n",
            x + 12.0,
            y + 22.0,
            x + 12.0,
            xml_escape(file_name),
            x + 12.0,
            xml_escape(&node.file_relative_path),
            x + 12.0,
            node.function_count
        ));
    }

    svg.push_str("</svg>\n");
    fs::write(path, svg)?;
    Ok(())
}

fn write_summary(path: &Path, summary: &GraphSummary) -> Result<(), Box<dyn Error>> {
    fs::write(path, serde_json::to_string_pretty(summary)?)?;
    Ok(())
}

fn run(args: &Args) -> Result<(), Box<dyn Error>> {
    let bytes = fs::read(&args.input_path)?;
    let index = Index::parse_from_bytes(&bytes)?;
    let result = build_graph(&index, args)?;
    let overview = build_overview_graph(&result);

    fs::create_dir_all(&args.output_dir)?;
    write_dot(&args.output_dir.join("backend_core.dot"), &result)?;
    write_svg(&args.output_dir.join("backend_core.simple.svg"), &result)?;
    write_overview_dot(&args.output_dir.join("backend_core.overview.dot"), &overview)?;
    write_overview_svg(
        &args.output_dir.join("backend_core.overview.simple.svg"),
        &overview,
    )?;
    write_summary(
        &args.output_dir.join("backend_core.summary.json"),
        &build_summary(&result),
    )?;
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let parsed = match parse_args(&args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    };

    if let Err(error) = run(&parsed) {
        eprintln!("failed to generate backend call graph: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protobuf::{EnumOrUnknown, MessageField};
    use scip::types::{
        Metadata, PositionEncoding, SymbolInformation, TextEncoding, ToolInfo,
        symbol_information,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    fn build_test_index(project_root: &Path) -> Index {
        let mut root_symbol = SymbolInformation::new();
        root_symbol.symbol = String::from("rust-analyzer cargo game_api 0.1.0 app/root().");
        root_symbol.kind = EnumOrUnknown::new(symbol_information::Kind::Function);
        root_symbol.display_name = String::from("root");

        let mut helper_symbol = SymbolInformation::new();
        helper_symbol.symbol = String::from("rust-analyzer cargo game_api 0.1.0 app/helper().");
        helper_symbol.kind = EnumOrUnknown::new(symbol_information::Kind::Function);
        helper_symbol.display_name = String::from("helper");

        let mut enum_symbol = SymbolInformation::new();
        enum_symbol.symbol = String::from("rust-analyzer cargo game_api 0.1.0 app/RoundWon.");
        enum_symbol.kind = EnumOrUnknown::new(symbol_information::Kind::EnumMember);
        enum_symbol.display_name = String::from("RoundWon");

        let mut test_symbol = SymbolInformation::new();
        test_symbol.symbol =
            String::from("rust-analyzer cargo game_api 0.1.0 app/tests/root_test().");
        test_symbol.kind = EnumOrUnknown::new(symbol_information::Kind::Function);
        test_symbol.display_name = String::from("root_test");

        let definition = |line: i32, symbol: &str| {
            let mut occurrence = Occurrence::new();
            occurrence.range = vec![line, 0, line, 4];
            occurrence.symbol = symbol.to_string();
            occurrence.symbol_roles = 1;
            occurrence
        };
        let reference = |line: i32, symbol: &str| {
            let mut occurrence = Occurrence::new();
            occurrence.range = vec![line, 4, line, 10];
            occurrence.symbol = symbol.to_string();
            occurrence
        };

        let mut document = Document::new();
        document.language = String::from("rust");
        document.relative_path = String::from("crates/game_api/src/app.rs");
        document.position_encoding =
            EnumOrUnknown::new(PositionEncoding::UTF8CodeUnitOffsetFromLineStart);
        document.symbols = vec![root_symbol, helper_symbol, enum_symbol, test_symbol];
        document.occurrences = vec![
            definition(0, "rust-analyzer cargo game_api 0.1.0 app/root()."),
            reference(1, "rust-analyzer cargo game_api 0.1.0 app/helper()."),
            reference(
                2,
                "rust-analyzer cargo core https://github.com/rust-lang/rust/library/core option/impl#[`Option<T>`]unwrap_or_else().",
            ),
            reference(3, "rust-analyzer cargo game_api 0.1.0 app/RoundWon."),
            definition(6, "rust-analyzer cargo game_api 0.1.0 app/helper()."),
            definition(11, "rust-analyzer cargo game_api 0.1.0 app/tests/root_test()."),
            reference(12, "rust-analyzer cargo game_api 0.1.0 app/helper()."),
        ];

        let mut tool_info = ToolInfo::new();
        tool_info.name = String::from("rust-analyzer");
        tool_info.version = String::from("test");

        let mut metadata = Metadata::new();
        metadata.tool_info = MessageField::some(tool_info);
        metadata.project_root = format!("file://{}", project_root.display());
        metadata.text_document_encoding = EnumOrUnknown::new(TextEncoding::UTF8);

        let mut index = Index::new();
        index.metadata = MessageField::some(metadata);
        index.documents.push(document);
        index
    }

    fn unique_temp_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("rarena-backend-callgraph-{unique}"))
    }

    fn write_test_source(project_root: &Path) {
        let source_path = source_file_path(project_root, "crates/game_api/src/app.rs");
        fs::create_dir_all(
            source_path
                .parent()
                .expect("test source path should have a parent directory"),
        )
        .expect("source directory should be created");
        fs::write(
            source_path,
            "fn root() {\n    helper();\n    option.unwrap_or_else();\n    RoundWon;\n}\n\nfn helper() {\n}\n\nmod tests {\n    #[test]\n    fn root_test() {\n        helper();\n    }\n}\n",
        )
        .expect("test source should be written");
    }

    #[test]
    fn parse_args_accepts_backend_and_entry_files() {
        let args = vec![
            String::from("backend_callgraph"),
            String::from("index.scip"),
            String::from("out"),
            String::from("--backend-file"),
            String::from("crates/game_api/src/app.rs"),
            String::from("--entry-file"),
            String::from("crates/game_api/src/app.rs"),
        ];

        let parsed = parse_args(&args).expect("arguments should parse");
        assert_eq!(parsed.input_path, PathBuf::from("index.scip"));
        assert!(parsed
            .backend_files
            .contains("crates/game_api/src/app.rs"));
        assert!(parsed.entry_files.contains("crates/game_api/src/app.rs"));
    }

    #[test]
    fn parse_args_rejects_missing_flags_and_values() {
        let missing_backend = vec![
            String::from("backend_callgraph"),
            String::from("index.scip"),
            String::from("out"),
            String::from("--entry-file"),
            String::from("crates/game_api/src/app.rs"),
        ];
        assert!(parse_args(&missing_backend).is_err());

        let missing_value = vec![
            String::from("backend_callgraph"),
            String::from("index.scip"),
            String::from("out"),
            String::from("--backend-file"),
        ];
        assert!(parse_args(&missing_value).is_err());
    }

    #[test]
    fn callable_kinds_accept_only_functions_and_methods() {
        assert!(is_callable_kind(KIND_FUNCTION));
        assert!(is_callable_kind(KIND_METHOD));
        assert!(is_callable_kind(KIND_STATIC_METHOD));
        assert!(!is_callable_kind(12));
        assert!(!is_callable_kind(6));
        assert!(!is_callable_kind(29));
    }

    #[test]
    fn normalize_symbol_name_formats_methods_and_functions() {
        assert_eq!(
            normalize_symbol_name("rust-analyzer cargo game_api 0.1.0 app/ServerApp#handle_packet()."),
            "game_api::app::ServerApp::handle_packet"
        );
        assert_eq!(
            normalize_symbol_name("rust-analyzer cargo game_api 0.1.0 app/spawn_dev_server()."),
            "game_api::app::spawn_dev_server"
        );
    }

    #[test]
    fn extract_function_span_requires_a_real_body() {
        let lines = vec![
            String::from("fn a() {"),
            String::from("    helper();"),
            String::from("}"),
            String::from("fn declaration();"),
        ];

        let span = extract_function_span(&lines, 0).expect("body should be found");
        assert_eq!(span.body_start_line, 0);
        assert_eq!(span.end_line, 2);
        assert!(extract_function_span(&lines, 3).is_none());
    }

    #[test]
    fn test_detection_finds_inline_unit_tests() {
        let lines = vec![
            String::from("mod tests {"),
            String::from("    #[test]"),
            String::from("    fn helper_test() {"),
        ];
        assert!(is_test_function("game_api::tests::helper_test", &lines, 2));
        assert!(!is_test_function("game_api::app::helper", &lines, 0));
    }

    #[test]
    fn build_graph_filters_tests_and_enum_members_and_tracks_local_edges() -> Result<(), String> {
        let project_root = unique_temp_dir();
        write_test_source(&project_root);
        let index = build_test_index(&project_root);
        let args = Args {
            input_path: PathBuf::from("index.scip"),
            output_dir: PathBuf::from("out"),
            backend_files: BTreeSet::from([String::from("crates/game_api/src/app.rs")]),
            entry_files: BTreeSet::from([String::from("crates/game_api/src/app.rs")]),
        };

        let result = build_graph(&index, &args)?;
        let summary = build_summary(&result);

        assert_eq!(summary.node_count, 2);
        assert_eq!(summary.edge_count, 1);
        assert_eq!(summary.omitted_test_nodes, 1);
        assert_eq!(summary.omitted_bodyless_nodes, 0);
        assert_eq!(
            summary.roots,
            vec![String::from("game_api::app::root")]
        );
        assert_eq!(
            summary.hidden_external_references,
            vec![ExternalReference {
                crate_name: String::from("core"),
                count: 1
            }]
        );

        let root = result
            .nodes
            .values()
            .find(|node| node.short_label.ends_with("root"))
            .expect("root node should exist");
        assert!(root
            .outgoing
            .contains("rust-analyzer cargo game_api 0.1.0 app/helper()."));
        assert!(result
            .nodes
            .values()
            .all(|node| !node.short_label.contains("RoundWon")));

        fs::remove_dir_all(&project_root)
            .map_err(|error| format!("cleanup should succeed: {error}"))?;
        Ok(())
    }

    #[test]
    fn overview_graph_aggregates_cross_file_edges() {
        let mut root_node = Node {
            symbol: String::from("root"),
            full_name: String::from("game_api::realtime::spawn_dev_server"),
            short_label: String::from("spawn_dev_server"),
            file_relative_path: String::from("crates/game_api/src/realtime.rs"),
            start_line: 0,
            body_start_line: 0,
            end_line: 4,
            outgoing: BTreeSet::from([String::from("handle"), String::from("validate")]),
            incoming: BTreeSet::new(),
        };
        let mut handle_node = Node {
            symbol: String::from("handle"),
            full_name: String::from("game_api::app::ServerApp::handle_packet"),
            short_label: String::from("handle_packet"),
            file_relative_path: String::from("crates/game_api/src/app.rs"),
            start_line: 8,
            body_start_line: 8,
            end_line: 16,
            outgoing: BTreeSet::new(),
            incoming: BTreeSet::from([String::from("root")]),
        };
        let validate_node = Node {
            symbol: String::from("validate"),
            full_name: String::from("game_net::ingress::validate_packet"),
            short_label: String::from("validate_packet"),
            file_relative_path: String::from("crates/game_net/src/ingress.rs"),
            start_line: 5,
            body_start_line: 5,
            end_line: 14,
            outgoing: BTreeSet::new(),
            incoming: BTreeSet::from([String::from("root")]),
        };
        root_node.incoming = BTreeSet::new();
        handle_node.outgoing = BTreeSet::from([String::from("validate")]);

        let result = BuildResult {
            nodes: BTreeMap::from([
                (String::from("root"), root_node),
                (String::from("handle"), handle_node),
                (String::from("validate"), validate_node),
            ]),
            roots: vec![String::from("root")],
            omitted_test_nodes: 0,
            omitted_bodyless_nodes: 0,
            omitted_unreachable_nodes: 0,
            hidden_external_references: Vec::new(),
        };

        let overview = build_overview_graph(&result);
        let summary = build_summary(&result);
        let api_realtime = overview
            .nodes
            .get("crates/game_api/src/realtime.rs")
            .expect("realtime file should be present");
        assert!(api_realtime.is_root);
        assert_eq!(api_realtime.function_count, 1);
        assert_eq!(api_realtime.outgoing["crates/game_api/src/app.rs"], 1);
        assert_eq!(api_realtime.outgoing["crates/game_net/src/ingress.rs"], 1);
        assert_eq!(summary.overview_file_count, 3);
        assert_eq!(summary.overview_edge_count, 3);
        assert_eq!(
            summary.top_file_edges[0],
            RankedFileEdge {
                source_file: String::from("crates/game_api/src/app.rs"),
                target_file: String::from("crates/game_net/src/ingress.rs"),
                count: 1,
            }
        );
    }

    #[test]
    fn svg_output_escapes_xml_sensitive_labels() -> Result<(), Box<dyn Error>> {
        let output_dir = unique_temp_dir();
        fs::create_dir_all(&output_dir)?;

        let mut nodes = BTreeMap::new();
        nodes.insert(
            String::from("a"),
            Node {
                symbol: String::from("a"),
                full_name: String::from("game_api::app::impl#[Option<Self>]::call"),
                short_label: String::from("impl#[Option<Self>]::call"),
                file_relative_path: String::from("crates/game_api/src/app.rs"),
                start_line: 9,
                body_start_line: 9,
                end_line: 11,
                outgoing: BTreeSet::new(),
                incoming: BTreeSet::new(),
            },
        );
        let result = BuildResult {
            nodes,
            roots: vec![String::from("a")],
            omitted_test_nodes: 0,
            omitted_bodyless_nodes: 0,
            omitted_unreachable_nodes: 0,
            hidden_external_references: Vec::new(),
        };

        let svg_path = output_dir.join("backend_core.simple.svg");
        write_svg(&svg_path, &result)?;
        let svg = fs::read_to_string(&svg_path)?;
        assert!(svg.contains("impl#[Option&lt;Self&gt;]::call"));

        fs::remove_dir_all(&output_dir)?;
        Ok(())
    }

    #[test]
    fn overview_svg_escapes_xml_sensitive_labels() -> Result<(), Box<dyn Error>> {
        let output_dir = unique_temp_dir();
        fs::create_dir_all(&output_dir)?;

        let overview = OverviewGraph {
            nodes: BTreeMap::from([(
                String::from("crates/game_api/src/impl<Option<Self>>.rs"),
                OverviewNode {
                    file_relative_path: String::from(
                        "crates/game_api/src/impl<Option<Self>>.rs",
                    ),
                    function_count: 3,
                    outgoing: BTreeMap::new(),
                    incoming: BTreeMap::new(),
                    is_root: true,
                },
            )]),
        };

        let svg_path = output_dir.join("backend_core.overview.simple.svg");
        write_overview_svg(&svg_path, &overview)?;
        let svg = fs::read_to_string(&svg_path)?;
        assert!(svg.contains("impl&lt;Option&lt;Self&gt;&gt;.rs"));

        fs::remove_dir_all(&output_dir)?;
        Ok(())
    }
}

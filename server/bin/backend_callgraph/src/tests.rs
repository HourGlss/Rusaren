use super::*;
use protobuf::{EnumOrUnknown, MessageField};
use scip::types::{
    symbol_information, Metadata, PositionEncoding, SymbolInformation, TextEncoding, ToolInfo,
};
use std::time::{SystemTime, UNIX_EPOCH};

fn build_test_index(project_root: &Path) -> Index {
    let mut root_symbol = SymbolInformation::new();
    root_symbol.symbol = String::from("rust-analyzer cargo game_api 0.8.0 app/root().");
    root_symbol.kind = EnumOrUnknown::new(symbol_information::Kind::Function);
    root_symbol.display_name = String::from("root");

    let mut helper_symbol = SymbolInformation::new();
    helper_symbol.symbol = String::from("rust-analyzer cargo game_api 0.8.0 app/helper().");
    helper_symbol.kind = EnumOrUnknown::new(symbol_information::Kind::Function);
    helper_symbol.display_name = String::from("helper");

    let mut enum_symbol = SymbolInformation::new();
    enum_symbol.symbol = String::from("rust-analyzer cargo game_api 0.8.0 app/RoundWon.");
    enum_symbol.kind = EnumOrUnknown::new(symbol_information::Kind::EnumMember);
    enum_symbol.display_name = String::from("RoundWon");

    let mut test_symbol = SymbolInformation::new();
    test_symbol.symbol = String::from("rust-analyzer cargo game_api 0.8.0 app/tests/root_test().");
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
            definition(0, "rust-analyzer cargo game_api 0.8.0 app/root()."),
            reference(1, "rust-analyzer cargo game_api 0.8.0 app/helper()."),
            reference(
                2,
                "rust-analyzer cargo core https://github.com/rust-lang/rust/library/core option/impl#[`Option<T>`]unwrap_or_else().",
            ),
            reference(3, "rust-analyzer cargo game_api 0.8.0 app/RoundWon."),
            definition(6, "rust-analyzer cargo game_api 0.8.0 app/helper()."),
            definition(11, "rust-analyzer cargo game_api 0.8.0 app/tests/root_test()."),
            reference(12, "rust-analyzer cargo game_api 0.8.0 app/helper()."),
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
    assert!(parsed.backend_files.contains("crates/game_api/src/app.rs"));
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
        normalize_symbol_name("rust-analyzer cargo game_api 0.8.0 app/ServerApp#handle_packet()."),
        "game_api::app::ServerApp::handle_packet"
    );
    assert_eq!(
        normalize_symbol_name("rust-analyzer cargo game_api 0.8.0 app/spawn_dev_server()."),
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
    assert_eq!(summary.roots, vec![String::from("game_api::app::root")]);
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
        .contains("rust-analyzer cargo game_api 0.8.0 app/helper()."));
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
                file_relative_path: String::from("crates/game_api/src/impl<Option<Self>>.rs"),
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

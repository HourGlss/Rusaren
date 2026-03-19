use super::*;
use protobuf::{EnumOrUnknown, MessageField};
use scip::types::{symbol_information, PositionEncoding, TextEncoding};
use serde_json::Value;

fn build_minimal_index() -> Index {
    let mut signature = Document::new();
    signature.language = String::from("rust");
    signature.text = String::from("fn sample(a: i32) -> i32");
    signature.position_encoding =
        EnumOrUnknown::new(PositionEncoding::UTF8CodeUnitOffsetFromLineStart);

    let mut symbol = SymbolInformation::new();
    symbol.symbol = String::from("rust-analyzer cargo sample 0.8.0 sample_fn().");
    symbol.kind = EnumOrUnknown::new(symbol_information::Kind::Function);
    symbol.display_name = String::from("sample_fn");
    symbol.documentation.push(String::from("example symbol"));
    symbol.signature_documentation = MessageField::some(signature);

    let mut occurrence = Occurrence::new();
    occurrence.range = vec![0, 0, 0, 8];
    occurrence.symbol = String::from("rust-analyzer cargo sample 0.8.0 sample_fn().");
    occurrence.symbol_roles = 1;

    let mut document = Document::new();
    document.language = String::from("rust");
    document.relative_path = String::from("src/lib.rs");
    document.position_encoding =
        EnumOrUnknown::new(PositionEncoding::UTF8CodeUnitOffsetFromLineStart);
    document.occurrences.push(occurrence);
    document.symbols.push(symbol);

    let mut tool_info = ToolInfo::new();
    tool_info.name = String::from("rust-analyzer");
    tool_info.version = String::from("test");

    let mut metadata = Metadata::new();
    metadata.tool_info = MessageField::some(tool_info);
    metadata.project_root = String::from("file:///workspace");
    metadata.text_document_encoding = EnumOrUnknown::new(TextEncoding::UTF8);

    let mut index = Index::new();
    index.metadata = MessageField::some(metadata);
    index.documents.push(document);
    index
}

#[test]
fn parse_paths_accepts_exactly_two_file_arguments() {
    let args = vec![
        String::from("scip_json_dump"),
        String::from("index.scip"),
        String::from("index.scip.json"),
    ];

    let (input, output) = parse_paths(&args).expect("paths should parse");
    assert_eq!(input, PathBuf::from("index.scip"));
    assert_eq!(output, PathBuf::from("index.scip.json"));
}

#[test]
fn parse_paths_rejects_missing_and_extra_arguments() {
    let missing = vec![String::from("scip_json_dump"), String::from("index.scip")];
    assert!(parse_paths(&missing).is_err());

    let extra = vec![
        String::from("scip_json_dump"),
        String::from("index.scip"),
        String::from("index.scip.json"),
        String::from("extra"),
    ];
    assert!(parse_paths(&extra).is_err());
}

#[test]
fn dump_scip_bytes_to_json_serializes_the_expected_callgraph_shape() {
    let index = build_minimal_index();
    let bytes = index
        .write_to_bytes()
        .expect("protobuf bytes should encode");

    let json = dump_scip_bytes_to_json(&bytes).expect("json conversion should succeed");
    let parsed: Value = serde_json::from_str(&json).expect("json should parse");

    assert_eq!(parsed["metadata"]["tool_info"]["name"], "rust-analyzer");
    assert_eq!(parsed["metadata"]["text_document_encoding"], 1);
    assert_eq!(parsed["documents"][0]["relative_path"], "src/lib.rs");
    assert_eq!(parsed["documents"][0]["occurrences"][0]["symbol_roles"], 1);
    assert_eq!(parsed["documents"][0]["symbols"][0]["kind"], 17);
    assert_eq!(
        parsed["documents"][0]["symbols"][0]["signature_documentation"]["language"],
        "rust"
    );
}

#[test]
fn dump_scip_bytes_to_json_omits_optional_fields_when_they_are_empty() {
    let index = Index::new();
    let bytes = index
        .write_to_bytes()
        .expect("protobuf bytes should encode");

    let json = dump_scip_bytes_to_json(&bytes).expect("json conversion should succeed");
    let parsed: Value = serde_json::from_str(&json).expect("json should parse");

    assert!(parsed["metadata"]["tool_info"]["name"].as_str().is_some());
    assert!(parsed["documents"].as_array().is_some());
}

#[test]
fn dump_scip_bytes_to_json_rejects_invalid_binary_input() {
    let invalid = [0_u8, 1, 2, 3, 4, 5];

    assert!(dump_scip_bytes_to_json(&invalid).is_err());
}

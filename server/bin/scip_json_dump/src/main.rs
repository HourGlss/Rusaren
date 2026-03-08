//! Converts a binary `index.scip` file into the JSON format expected by
//! `scip-callgraph`.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use protobuf::Message;
use scip::types::{Document, Index, Metadata, Occurrence, SymbolInformation, ToolInfo};
use serde::Serialize;

#[derive(Serialize)]
struct ScipIndexJson {
    metadata: MetadataJson,
    documents: Vec<DocumentJson>,
}

#[derive(Serialize)]
struct MetadataJson {
    tool_info: ToolInfoJson,
    project_root: String,
    text_document_encoding: i32,
}

#[derive(Serialize)]
struct ToolInfoJson {
    name: String,
    version: String,
}

#[derive(Serialize)]
struct DocumentJson {
    language: String,
    relative_path: String,
    occurrences: Vec<OccurrenceJson>,
    symbols: Vec<SymbolJson>,
    position_encoding: i32,
}

#[derive(Serialize)]
struct OccurrenceJson {
    range: Vec<i32>,
    symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    symbol_roles: Option<i32>,
}

#[derive(Serialize)]
struct SymbolJson {
    symbol: String,
    kind: i32,
    display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    documentation: Option<Vec<String>>,
    signature_documentation: SignatureDocumentationJson,
    #[serde(skip_serializing_if = "Option::is_none")]
    enclosing_symbol: Option<String>,
}

#[derive(Serialize)]
struct SignatureDocumentationJson {
    language: String,
    text: String,
    position_encoding: i32,
}

fn parse_paths(args: &[String]) -> Result<(PathBuf, PathBuf), String> {
    if args.len() != 3 {
        return Err(format!("usage: {} <input-index.scip> <output-index.scip.json>", args[0]));
    }

    Ok((PathBuf::from(&args[1]), PathBuf::from(&args[2])))
}

fn normalize_index(index: Index) -> ScipIndexJson {
    let metadata = normalize_metadata(index.metadata.into_option().unwrap_or_default());
    let documents = index.documents.into_iter().map(normalize_document).collect();

    ScipIndexJson { metadata, documents }
}

fn normalize_metadata(metadata: Metadata) -> MetadataJson {
    MetadataJson {
        tool_info: normalize_tool_info(metadata.tool_info.into_option().unwrap_or_default()),
        project_root: metadata.project_root,
        text_document_encoding: metadata.text_document_encoding.value(),
    }
}

fn normalize_tool_info(tool_info: ToolInfo) -> ToolInfoJson {
    ToolInfoJson {
        name: tool_info.name,
        version: tool_info.version,
    }
}

fn normalize_document(document: Document) -> DocumentJson {
    DocumentJson {
        language: document.language,
        relative_path: document.relative_path,
        occurrences: document
            .occurrences
            .into_iter()
            .map(normalize_occurrence)
            .collect(),
        symbols: document.symbols.into_iter().map(normalize_symbol).collect(),
        position_encoding: document.position_encoding.value(),
    }
}

fn normalize_occurrence(occurrence: Occurrence) -> OccurrenceJson {
    OccurrenceJson {
        range: occurrence.range,
        symbol: occurrence.symbol,
        symbol_roles: if occurrence.symbol_roles == 0 {
            None
        } else {
            Some(occurrence.symbol_roles)
        },
    }
}

fn normalize_symbol(symbol: SymbolInformation) -> SymbolJson {
    SymbolJson {
        symbol: symbol.symbol,
        kind: symbol.kind.value(),
        display_name: if symbol.display_name.is_empty() {
            None
        } else {
            Some(symbol.display_name)
        },
        documentation: if symbol.documentation.is_empty() {
            None
        } else {
            Some(symbol.documentation)
        },
        signature_documentation: normalize_signature_documentation(
            symbol.signature_documentation.into_option().unwrap_or_default(),
        ),
        enclosing_symbol: if symbol.enclosing_symbol.is_empty() {
            None
        } else {
            Some(symbol.enclosing_symbol)
        },
    }
}

fn normalize_signature_documentation(document: Document) -> SignatureDocumentationJson {
    SignatureDocumentationJson {
        language: document.language,
        text: document.text,
        position_encoding: document.position_encoding.value(),
    }
}

fn dump_scip_bytes_to_json(bytes: &[u8]) -> Result<String, Box<dyn Error>> {
    let index = Index::parse_from_bytes(bytes)?;
    let normalized = normalize_index(index);
    Ok(serde_json::to_string(&normalized)?)
}

fn run(input_path: &Path, output_path: &Path) -> Result<(), Box<dyn Error>> {
    let bytes = fs::read(input_path)?;
    let json = dump_scip_bytes_to_json(&bytes)?;
    fs::write(output_path, json)?;
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (input_path, output_path) = match parse_paths(&args) {
        Ok(paths) => paths,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    };

    if let Err(error) = run(&input_path, &output_path) {
        eprintln!(
            "failed to convert {} to {}: {error}",
            input_path.display(),
            output_path.display()
        );
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protobuf::{EnumOrUnknown, MessageField};
    use scip::types::{PositionEncoding, TextEncoding, symbol_information};
    use serde_json::Value;

    fn build_minimal_index() -> Index {
        let mut signature = Document::new();
        signature.language = String::from("rust");
        signature.text = String::from("fn sample(a: i32) -> i32");
        signature.position_encoding =
            EnumOrUnknown::new(PositionEncoding::UTF8CodeUnitOffsetFromLineStart);

        let mut symbol = SymbolInformation::new();
        symbol.symbol = String::from("rust-analyzer cargo sample 0.1.0 sample_fn().");
        symbol.kind = EnumOrUnknown::new(symbol_information::Kind::Function);
        symbol.display_name = String::from("sample_fn");
        symbol.documentation.push(String::from("example symbol"));
        symbol.signature_documentation = MessageField::some(signature);

        let mut occurrence = Occurrence::new();
        occurrence.range = vec![0, 0, 0, 8];
        occurrence.symbol = String::from("rust-analyzer cargo sample 0.1.0 sample_fn().");
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
        let bytes = index.write_to_bytes().expect("protobuf bytes should encode");

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
        let bytes = index.write_to_bytes().expect("protobuf bytes should encode");

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
}

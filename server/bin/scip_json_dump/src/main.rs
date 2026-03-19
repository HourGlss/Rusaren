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
        return Err(format!(
            "usage: {} <input-index.scip> <output-index.scip.json>",
            args[0]
        ));
    }

    Ok((PathBuf::from(&args[1]), PathBuf::from(&args[2])))
}

fn normalize_index(index: Index) -> ScipIndexJson {
    let metadata = normalize_metadata(index.metadata.into_option().unwrap_or_default());
    let documents = index
        .documents
        .into_iter()
        .map(normalize_document)
        .collect();

    ScipIndexJson {
        metadata,
        documents,
    }
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
            symbol
                .signature_documentation
                .into_option()
                .unwrap_or_default(),
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
mod tests;

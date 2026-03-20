use tower_lsp::lsp_types::*;
use crate::resolution::{AnalyzedDoc, span_at_position_with_db, span_to_location, name_len_at};

pub fn get_definition(doc: &AnalyzedDoc, uri: &Url, position: Position) -> Option<Location> {
    let span = span_at_position_with_db(doc, position)?;
    let def  = doc.db.resolve_def(span)?;
    let len  = name_len_at(&doc.db, def);
    Some(span_to_location(def, len, uri))
}

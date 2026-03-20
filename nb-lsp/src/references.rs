use tower_lsp::lsp_types::*;
use crate::resolution::{AnalyzedDoc, span_at_position_with_db, span_to_location, name_len_at};

pub fn get_references(doc: &AnalyzedDoc, uri: &Url, position: Position) -> Vec<Location> {
    let Some(span) = span_at_position_with_db(doc, position) else { return vec![]; };
    let uses = doc.db.find_references(span);
    uses.into_iter()
        .map(|s| {
            let len = name_len_at(&doc.db, doc.db.use_to_def.get(&s).copied().unwrap_or(s));
            span_to_location(s, len, uri)
        })
        .collect()
}

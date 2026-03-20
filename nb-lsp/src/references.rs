use tower_lsp::lsp_types::*;
use crate::resolution::{build_resolution_db, span_at_position, span_to_location, name_len_at};

pub fn get_references(source: &str, uri: &Url, position: Position) -> Vec<Location> {
    let Some(db)   = build_resolution_db(source) else { return vec![]; };
    let Some(span) = span_at_position(source, position) else { return vec![]; };
    let uses = db.find_references(span);
    uses.into_iter()
        .map(|s| {
            let len = name_len_at(&db, db.use_to_def.get(&s).copied().unwrap_or(s));
            span_to_location(s, len, uri)
        })
        .collect()
}

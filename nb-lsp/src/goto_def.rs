use tower_lsp::lsp_types::*;
use crate::resolution::{build_resolution_db, span_at_position_with_db, span_to_location, name_len_at};

pub fn get_definition(source: &str, uri: &Url, position: Position) -> Option<Location> {
    let db   = build_resolution_db(source)?;
    let span = span_at_position_with_db(&db, source, position)?;
    let def  = db.resolve_def(span)?;
    let len  = name_len_at(&db, def);
    Some(span_to_location(def, len, uri))
}

use std::collections::HashMap;
use tower_lsp::lsp_types::*;
use crate::resolution::{build_resolution_db, span_at_position_with_db, span_to_range, name_len_at};

pub fn get_rename(source: &str, uri: &Url, position: Position, new_name: &str) -> Option<WorkspaceEdit> {
    let db   = build_resolution_db(source)?;
    let span = span_at_position_with_db(&db, source, position)?;
    let all  = db.find_all_occurrences(span);
    if all.is_empty() { return None; }

    let def = db.use_to_def.get(&span).copied().unwrap_or(span);
    let len = name_len_at(&db, def);

    let edits: Vec<TextEdit> = all.into_iter().map(|s| TextEdit {
        range: span_to_range(&s, len),
        new_text: new_name.to_string(),
    }).collect();

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);
    Some(WorkspaceEdit { changes: Some(changes), ..Default::default() })
}

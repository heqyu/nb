use std::collections::HashMap;
use tower_lsp::lsp_types::*;
use crate::resolution::{AnalyzedDoc, span_at_position_with_db, span_to_range, name_len_at};

pub fn get_rename(doc: &AnalyzedDoc, uri: &Url, position: Position, new_name: &str) -> Option<WorkspaceEdit> {
    let span = span_at_position_with_db(doc, position)?;
    let all  = doc.db.find_all_occurrences(span);
    if all.is_empty() { return None; }

    let def = doc.db.use_to_def.get(&span).copied().unwrap_or(span);
    let len = name_len_at(&doc.db, def);

    let edits: Vec<TextEdit> = all.into_iter().map(|s| TextEdit {
        range: span_to_range(&s, len),
        new_text: new_name.to_string(),
    }).collect();

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);
    Some(WorkspaceEdit { changes: Some(changes), ..Default::default() })
}

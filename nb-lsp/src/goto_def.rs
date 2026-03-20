use tower_lsp::lsp_types::*;

use crate::symbol_table::{build_table, ident_at_position, span_to_lsp_range};

pub fn get_definition(source: &str, uri: &Url, position: Position) -> Option<Location> {
    let cursor_name = ident_at_position(source, position)?;
    let table = build_table(source)?;

    // 光标已经在定义处 → 仍然跳到该定义（幂等）
    // 光标在使用处 → 按名字找定义
    let entry = table.lookup_at(position)
        .or_else(|| table.lookup_by_name(&cursor_name))?;

    let name_len = entry.info.name().len() as u32;
    let range = span_to_lsp_range(&entry.def_span, name_len);

    Some(Location {
        uri: uri.clone(),
        range,
    })
}

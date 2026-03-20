mod diagnostics;
mod semantic;
mod symbol_table;
mod symbols;
mod hover;
mod goto_def;
mod backend;

use tower_lsp::{LspService, Server};
use backend::Backend;

#[tokio::main]
async fn main() {
    let stdin  = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}

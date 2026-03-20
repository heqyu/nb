use std::sync::Arc;
use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::completion::get_completions;
use crate::diagnostics::get_diagnostics;
use crate::goto_def::get_definition;
use crate::hover::get_hover;
use crate::references::get_references;
use crate::rename::get_rename;
use crate::semantic::{get_semantic_tokens, semantic_token_legend};
use crate::signature::get_signature_help;
use crate::symbols::get_document_symbols;

pub struct Backend {
    client: Client,
    /// 文档内容缓存：uri → 源码
    documents: Arc<DashMap<String, String>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(DashMap::new()),
        }
    }

    async fn on_change(&self, uri: Url, text: String) {
        self.documents.insert(uri.to_string(), text.clone());
        let diagnostics = get_diagnostics(&text);
        self.client.publish_diagnostics(uri, diagnostics, None).await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // 全量文本同步
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                // 语义 token
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: semantic_token_legend(),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            range: None,
                            work_done_progress_options: Default::default(),
                        },
                    ),
                ),
                // 文档大纲
                document_symbol_provider: Some(OneOf::Left(true)),
                // Hover
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                // Go to Definition
                definition_provider: Some(OneOf::Left(true)),
                // Find References
                references_provider: Some(OneOf::Left(true)),
                // Rename
                rename_provider: Some(OneOf::Left(true)),
                // Signature Help（触发字符：'(' 和 ','）
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".into(), ",".into()]),
                    retrigger_characters: Some(vec![",".into()]),
                    work_done_progress_options: Default::default(),
                }),
                // 代码补全
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".into()]),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "nb-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "nb-lsp 已启动")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.on_change(params.text_document.uri, params.text_document.text).await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.pop() {
            self.on_change(params.text_document.uri, change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents.remove(&params.text_document.uri.to_string());
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri.to_string();
        if let Some(source) = self.documents.get(&uri) {
            let tokens = get_semantic_tokens(&source);
            Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: tokens,
            })))
        } else {
            Ok(None)
        }
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri.clone();
        let pos = params.text_document_position_params.position;
        if let Some(source) = self.documents.get(&uri.to_string()) {
            Ok(get_definition(&source, &uri, pos).map(GotoDefinitionResponse::Scalar))
        } else {
            Ok(None)
        }
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let pos = params.text_document_position.position;
        if let Some(source) = self.documents.get(&uri.to_string()) {
            let locs = get_references(&source, &uri, pos);
            Ok(if locs.is_empty() { None } else { Some(locs) })
        } else {
            Ok(None)
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri.to_string();
        let pos = params.text_document_position_params.position;
        if let Some(source) = self.documents.get(&uri) {
            Ok(get_hover(&source, pos))
        } else {
            Ok(None)
        }
    }

    async fn signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri.to_string();
        let pos = params.text_document_position_params.position;
        if let Some(source) = self.documents.get(&uri) {
            Ok(get_signature_help(&source, pos))
        } else {
            Ok(None)
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri.to_string();
        if let Some(source) = self.documents.get(&uri) {
            let symbols = get_document_symbols(&source);
            Ok(Some(DocumentSymbolResponse::Nested(symbols)))
        } else {
            Ok(None)
        }
    }

    async fn completion(
        &self,
        params: CompletionParams,
    ) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let pos = params.text_document_position.position;
        if let Some(source) = self.documents.get(&uri) {
            let items = get_completions(&source, pos);
            Ok(Some(CompletionResponse::Array(items)))
        } else {
            Ok(None)
        }
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let pos = params.text_document_position.position;
        let new_name = &params.new_name;
        if let Some(source) = self.documents.get(&uri.to_string()) {
            Ok(get_rename(&source, &uri, pos, new_name))
        } else {
            Ok(None)
        }
    }
}

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

mod composer;
mod packagist;

#[derive(Debug)]
struct Backend {
    client: Client,
}

struct TextDocumentItem {
    uri: Url,
    version: i32,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: None,
                }),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "composer server initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_save(&self, _params: DidSaveTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file opened!")
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file opened!")
            .await;

        self.on_change(TextDocumentItem {
            uri: params.text_document.uri,
            version: params.text_document.version,
        })
        .await
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file changed!")
            .await;

        self.on_change(TextDocumentItem {
            uri: params.text_document.uri,
            version: params.text_document.version,
        })
        .await
    }
}

impl Backend {
    async fn on_change(&self, params: TextDocumentItem) {
        let composer_file = composer::parse_file(params.uri.clone()).unwrap();
        let update_data = packagist::get_packages_info(composer_file.dependencies.clone()).await;

        let mut diagnostics: Vec<Diagnostic> = vec![];

        // Loop through "require".
        for item in composer_file.dependencies {
            // Packagist data.
            let packagist_data = update_data.get(&item.name).unwrap();

            let version = item.version.replace("\"", "");

            if let Some(version) = packagist::check_for_package_update(packagist_data, version) {
                let diagnostic = || -> Option<Diagnostic> {
                    Some(Diagnostic::new(
                        Range::new(
                            Position {
                                line: item.line,
                                character: 1,
                            },
                            Position {
                                line: 0,
                                character: 1,
                            },
                        ),
                        Some(DiagnosticSeverity::WARNING),
                        None,
                        None,
                        format!("Update available: {:?}", version),
                        None,
                        None,
                    ))
                }();

                diagnostics.push(diagnostic.unwrap());
            } else {
            }
        }

        self.client
            .publish_diagnostics(params.uri.clone(), diagnostics, Some(params.version))
            .await;
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(|client| Backend { client }).finish();
    Server::new(stdin, stdout, socket).serve(service).await;
}

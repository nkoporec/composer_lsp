use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use log::LevelFilter;
use futures::{stream, StreamExt};

mod composer;
mod packagist;

#[derive(Debug)]
struct Backend {
    client: Client,
}

struct TextDocumentItem {
    uri: Url,
    text: String,
    version: i32
}

const LOG_FILE: &str = "/home/nkoporec/personal/composer_lsp/lsp.log";

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

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
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
            text: params.text_document.text,
            version: params.text_document.version,
        })
        .await
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file changed!")
            .await;

        self.on_change(TextDocumentItem {
            uri: params.text_document.uri,
            text: std::mem::take(&mut params.content_changes[0].text),
            version: params.text_document.version,
        })
        .await
    }
}

impl Backend {
    async fn on_change(&self, params: TextDocumentItem) {
        let composer_file = composer::parse_file(params.uri.clone()).unwrap();
        let update_data = packagist::get_packages_info(composer_file.dependencies.clone()).await;
        log::info!("update_data {:#?}", update_data);

        let mut diagnostics = vec![];
        for item in composer_file.dependencies {
            // composer.json data.
            let name = item.name.replace(".", "");
            let version_normalized = item.version.replace(".", "");

            // Packagist data.
            let package_data = update_data.get(&name).unwrap();
            let new_version_normalized = &package_data.latest_version;

            // @todo implement version constraints.
            if new_version_normalized > &version_normalized {
                let diagnostic = || -> Option<Diagnostic> {
                    Some(Diagnostic::new_simple(
                        Range::new(Position { line: item.line, character: 1}, Position { line: 0, character: 1 }),
                        format!("Newest update {:?}", new_version_normalized),
                    ))
                }();

                diagnostics.push(diagnostic.unwrap());
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

    simple_logging::log_to_file(LOG_FILE, LevelFilter::Info);

    let (service, socket) = LspService::build(|client| Backend {
        client,
    }).finish();
    Server::new(stdin, stdout, socket).serve(service).await;
}

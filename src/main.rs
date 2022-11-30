use log::{error, info, warn};
use log4rs;
use std::{env, fmt::format, hash::Hash};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::composer::{ComposerFile, ComposerLock};

use std::{collections::HashMap, fs::File};

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
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
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
            .log_message(MessageType::INFO, "composer_lsp initialized!")
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let composer_file =
            composer::parse_json_file(uri.clone()).unwrap_or_else(|| ComposerFile {
                path: "".to_string(),
                dependencies: vec![],
                dev_dependencies: vec![],
            });

        // Get all dependencies and lines where they are defined.
        let mut items = HashMap::<u32, String>::new();
        for item in composer_file.dependencies {
            items.insert(item.line, item.name);
        }

        for item in composer_file.dev_dependencies {
            items.insert(item.line - 1, item.name);
        }

        let line = params.text_document_position_params.position.line;
        let dependency = items.get(&line);

        match dependency {
            Some(name) => {
                // @todo Make this version dependent, currently it pulls the data from the latest
                // version, but if we have a lock version, then we should pick that instead.
                let package_info = packagist::get_package_info(name.to_string()).await;
                match package_info {
                    Some(data) => {
                        let description_contents = MarkedString::from_markdown(data.description);
                        let new_line = MarkedString::from_markdown("".to_string());
                        let homepage_contents =
                            MarkedString::from_markdown(format!("Homepage: {}", data.homepage));

                        let authors_string = data.authors.join(",");
                        let authors_contents =
                            MarkedString::from_markdown(format!("Authors: {}", authors_string));

                        let contents = vec![
                            description_contents,
                            new_line,
                            authors_contents,
                            homepage_contents,
                        ];

                        let range = Range::new(
                            Position { line, character: 1 },
                            Position {
                                line: 0,
                                character: 1,
                            },
                        );

                        return Ok(Some(Hover {
                            contents: HoverContents::Array(contents),
                            range: Some(range),
                        }));
                    }
                    None => {
                        let error = format!("No hover data found for: {}", name);
                        log::error!("{}", error);
                        self.client.log_message(MessageType::ERROR, error).await;
                    }
                }
            }
            None => {
                let error = format!(
                    "Hover failed, because we can't find this line number: {}",
                    line
                );

                log::error!("{}", error);
                self.client.log_message(MessageType::ERROR, error).await;
            }
        }

        Ok(None)
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.on_save(TextDocumentItem {
            uri: params.text_document.uri,
            version: 1,
        })
        .await
    }
}

impl Backend {
    async fn on_save(&self, params: TextDocumentItem) {
        let composer_file =
            composer::parse_json_file(params.uri.clone()).unwrap_or_else(|| ComposerFile {
                path: "".to_string(),
                dependencies: vec![],
                dev_dependencies: vec![],
            });

        let mut composer_lock = ComposerLock {
            versions: HashMap::new(),
        };

        if composer_file.path != "" {
            match composer::parse_lock_file(&composer_file) {
                Some(lock) => {
                    composer_lock = lock;
                }
                None => {
                    info!("No lock file present.")
                }
            }
        }

        let update_data = packagist::get_packages_info(composer_file.dependencies.clone()).await;

        let mut diagnostics: Vec<Diagnostic> = vec![];

        // Loop through "require".
        for item in composer_file.dependencies {
            if item.name == "" {
                continue;
            }

            // Packagist data.
            let packagist_data = update_data.get(&item.name);
            match packagist_data {
                Some(package) => {
                    let mut composer_lock_version = "".to_string();

                    let composer_json_version = item.version.replace("\"", "");
                    if composer_lock.versions.len() > 0 {
                        let installed_package = composer_lock.versions.get(&item.name);
                        match installed_package {
                            Some(installed) => composer_lock_version = installed.version.clone(),
                            None => {}
                        }
                    }

                    if let Some(version) = packagist::check_for_package_update(
                        package,
                        composer_json_version,
                        composer_lock_version,
                    ) {
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
                    }
                }
                None => {}
            }
        }

        self.client
            .publish_diagnostics(params.uri.clone(), diagnostics, Some(params.version))
            .await;
    }
}

#[tokio::main]
async fn main() {
    match env::var("COMPOSER_LSP_LOG") {
        Ok(value) => {
            log4rs::init_file(value, Default::default()).unwrap();
            info!("LOG4RS logging enabled")
        }
        Err(_error) => {}
    }

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(|client| Backend { client }).finish();
    Server::new(stdin, stdout, socket).serve(service).await;
}

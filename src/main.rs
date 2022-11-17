use log::{info, LevelFilter};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::{
    composer::{ComposerDependency, ComposerFile, ComposerLock},
    packagist::Package,
};

use simplelog::*;
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
            composer_lock = composer::parse_lock_file(&composer_file).unwrap();
        }

        let update_data = packagist::get_packages_info(composer_file.dependencies.clone()).await;

        // Loop through "require".
        for item in composer_file.dependencies {
            let packagist_data = update_data.get(&item.name);
            match packagist_data {
                Some(package_data) => {
                    let diagnostics = get_updates(package_data, item, composer_lock.clone());
                    self.client
                        .publish_diagnostics(params.uri.clone(), diagnostics, Some(params.version))
                        .await;
                }
                None => {}
            }
        }

        // Loop through "require-dev".
        for item in composer_file.dev_dependencies {
            let packagist_data = update_data.get(&item.name);
            match packagist_data {
                Some(package_data) => {
                    let diagnostics = get_updates(package_data, item, composer_lock.clone());
                    self.client
                        .publish_diagnostics(params.uri.clone(), diagnostics, Some(params.version))
                        .await;
                }
                None => {}
            }
        }
    }
}

fn get_updates(
    package: &Package,
    dependency: ComposerDependency,
    composer_lock: ComposerLock,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = vec![];

    let mut composer_lock_version = "".to_string();

    let composer_json_version = dependency.version.replace("\"", "");
    if composer_lock.versions.len() > 0 {
        let installed_package = composer_lock.versions.get(&dependency.name);
        match installed_package {
            Some(installed) => composer_lock_version = installed.version.clone(),
            None => {}
        }
    }

    if let Some(version) =
        packagist::check_for_package_update(package, composer_json_version, composer_lock_version)
    {
        let diagnostic = || -> Option<Diagnostic> {
            Some(Diagnostic::new(
                Range::new(
                    Position {
                        line: dependency.line,
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

    diagnostics
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(|client| Backend { client }).finish();
    Server::new(stdin, stdout, socket).serve(service).await;
}

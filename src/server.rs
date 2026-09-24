use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{Peer, RoleServer, ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::config::Config;
use crate::delete::{self, DeleteRequest};
use crate::guard::ReadLog;
use crate::move_note::{self, MoveRequest};
use crate::save::{self, SaveRequest};
use crate::vault::Vault;
use crate::{change, index, project, review, search, usage};

pub struct Server {
    config: Config,
    // Saves rewrite the Index and commit: one at a time.
    write_lock: Mutex<()>,
    reads: ReadLog,
    #[expect(dead_code, reason = "read by the code #[tool_handler] generates")]
    tool_router: ToolRouter<Self>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ProjectDir {
    /// The agent's working directory, used to find the project.
    project_dir: Option<PathBuf>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ReadRequest {
    /// Note path as listed in the Index, e.g. `Preferences/commit-rules`.
    path: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchRequest {
    query: String,
    /// The agent's working directory, used to find the project.
    project_dir: Option<PathBuf>,
}

impl Server {
    fn vault(&self) -> Result<Vault, String> {
        Vault::load(&self.config.vault)
    }
}

fn agent_name(client: &Peer<RoleServer>) -> String {
    client
        .peer_info()
        .map(|info| info.client_info.name.clone())
        .unwrap_or_else(|| "unknown agent".to_string())
}

// Copilot CLI starts servers in the plugin folder, so the working directory is only a fallback.
fn project_key(vault: &Vault, dir: Option<PathBuf>) -> Option<String> {
    let dir = dir.or_else(|| std::env::current_dir().ok())?;
    project::resolve(&vault.projects, &dir).map(str::to_string)
}

#[tool_router]
impl Server {
    #[tool(
        description = "The memory Index: preferences and the current project's notes, one line each."
    )]
    fn memory_index(&self, Parameters(req): Parameters<ProjectDir>) -> Result<String, String> {
        let vault = self.vault()?;
        let project = project_key(&vault, req.project_dir);
        let view = index::for_project(&vault, project.as_deref());
        Ok(match view.trim() {
            "" => "no notes yet".to_string(),
            view => view.to_string(),
        })
    }

    #[tool(description = "Read one note.")]
    fn memory_read(&self, Parameters(req): Parameters<ReadRequest>) -> Result<String, String> {
        let vault = self.vault()?;
        let note = change::find_note(&vault, &req.path)?;
        let text = fs::read_to_string(vault.root.join(format!("{}.md", note.path)))
            .map_err(|e| e.to_string())?;
        self.reads.record(&note.path, &text);
        if let Err(e) = usage::record_read(&vault.root, &note.path, jiff::Zoned::now().date()) {
            eprintln!("petit-poucet: usage not recorded: {e}");
        }
        Ok(text)
    }

    #[tool(
        description = "Every note in one line (type, dates, reads, summary) plus the vault's check findings, for a memory cleanup review. Only for the memory-cleanup agent."
    )]
    fn memory_review(&self) -> Result<String, String> {
        review::review(&self.config)
    }

    #[tool(
        description = "Create or update one short fact. Search first; read a note before updating it; updating a `feedback` note needs the user's confirmation."
    )]
    fn memory_save(
        &self,
        Parameters(req): Parameters<SaveRequest>,
        client: Peer<RoleServer>,
    ) -> Result<String, String> {
        let _guard = self.write_lock.lock().map_err(|e| e.to_string())?;
        if let Some(path) = &req.path {
            self.reads.check(&self.config.vault, path)?;
        }
        let (path, reply) = save::save(&self.config, req, &agent_name(&client))?;
        self.reads.record_file(&self.config.vault, &path);
        Ok(reply)
    }

    #[tool(
        description = "Delete a wrong or obsolete note and unlink it everywhere. Deleting a `feedback` note needs the user's confirmation."
    )]
    fn memory_delete(
        &self,
        Parameters(req): Parameters<DeleteRequest>,
        client: Peer<RoleServer>,
    ) -> Result<String, String> {
        let _guard = self.write_lock.lock().map_err(|e| e.to_string())?;
        self.reads.check(&self.config.vault, &req.path)?;
        let path = req.path.trim_end_matches(".md").to_string();
        let (reply, rewritten) = delete::delete(&self.config, req, &agent_name(&client))?;
        self.reads.forget(&path);
        for file in &rewritten {
            self.reads.refresh(&self.config.vault, file);
        }
        usage::relocate(&self.config.vault, &path, None)?;
        Ok(reply)
    }

    #[tool(description = "Rename a note or move it to another scope, rewriting every link to it.")]
    fn memory_move(
        &self,
        Parameters(req): Parameters<MoveRequest>,
        client: Peer<RoleServer>,
    ) -> Result<String, String> {
        let _guard = self.write_lock.lock().map_err(|e| e.to_string())?;
        let path = req.path.trim_end_matches(".md").to_string();
        let new_path = req.new_path.trim_end_matches(".md").to_string();
        let (reply, rewritten) = move_note::move_note(&self.config, req, &agent_name(&client))?;
        self.reads.forget(&path);
        for file in &rewritten {
            self.reads.refresh(&self.config.vault, file);
        }
        usage::relocate(&self.config.vault, &path, Some(&new_path))?;
        Ok(reply)
    }

    #[tool(
        description = "Word search over titles, summaries and bodies of the preferences and the current project; best matches first."
    )]
    fn memory_search(&self, Parameters(req): Parameters<SearchRequest>) -> Result<String, String> {
        let vault = self.vault()?;
        let project = project_key(&vault, req.project_dir);
        let notes = vault
            .notes
            .iter()
            .filter(|n| n.project().is_none() || n.project() == project.as_deref());
        let found = search::search(notes, &req.query);
        if found.is_empty() {
            return Ok("no matches".to_string());
        }
        let lines: Vec<String> = found
            .iter()
            .map(|n| {
                format!(
                    "- [[{}]] — {}",
                    n.path,
                    n.summary().unwrap_or(index::NO_SUMMARY)
                )
            })
            .collect();
        Ok(lines.join("\n"))
    }
}

#[tool_handler]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_server_info(
            Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
        )
    }
}

pub fn serve(config: Config) -> Result<(), String> {
    Vault::load(&config.vault)?;
    let server = Server {
        config,
        write_lock: Mutex::new(()),
        reads: ReadLog::default(),
        tool_router: Server::tool_router(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let service = server
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| e.to_string())?;
        service.waiting().await.map_err(|e| e.to_string())?;
        Ok(())
    })
}

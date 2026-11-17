use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, RequestMetaObject, ServerCapabilities, ServerConfig};
use rmcp::{Peer, RoleServer, ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::config::Config;
use crate::delete::{self, DeleteRequest};
use crate::guard::ReadLog;
use crate::move_note::{self, MoveRequest};
use crate::note::Place;
use crate::review_session::ReviewSession;
use crate::save::{self, SaveRequest};
use crate::vault::{Vault, note_file};
use crate::{change, cleanup, context, index, lock, search, usage};

// Clients that don't run hooks (Copilot in JetBrains IDEs) never get the session-start context.
const INSTRUCTIONS: &str = "petit-poucet holds the user's memory: rules, decisions and verified facts. \
If your context has no \"Agent memory (petit-poucet)\" block, call memory_index with your working directory \
as project_dir before starting a task, and follow the rules it returns.";

pub struct Server {
    config: Config,
    reads: ReadLog,
    review: ReviewSession,
    #[expect(dead_code, reason = "read by the code #[tool_handler] generates")]
    tool_router: ToolRouter<Self>,
}

#[derive(Deserialize, JsonSchema)]
pub struct IndexRequest {
    /// The agent's working directory, used to find the project.
    project_dir: PathBuf,
    /// List this topic's notes instead, e.g. `homelab`.
    topic: Option<String>,
    /// Page to read, from 1. An Index longer than the session budget comes in pages; the first is the default.
    page: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ReadRequest {
    /// The note as listed in the Index: its name, e.g. `commit-rules`, or its full path.
    path: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ReviewRequest {
    /// Page to return, from 1 (the default); each page names the next one.
    page: Option<usize>,
    /// Every note, whatever the vault's size: only when the user asked to review their whole memory.
    full: Option<bool>,
    /// After the confirmed fixes are applied: marks the cleanup done, so they don't count towards the next reminder.
    done: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchRequest {
    query: String,
    /// The agent's working directory, used to find the project.
    project_dir: PathBuf,
}

impl Server {
    fn vault(&self) -> Result<Vault, String> {
        Vault::load(&self.config.vault)
    }

    fn lock(&self) -> Result<fs::File, String> {
        lock::vault(&self.config.vault)
    }

    // The path of the note a tool call names, which may be the short name the session view shows.
    fn resolve(&self, name: &str) -> Result<String, String> {
        Ok(change::find_note(&self.vault()?, name)?.path.clone())
    }
}

// Clients of protocol 2026-07-28, such as Claude Code, skip `initialize` and name themselves in each request.
fn agent_name(client: &Peer<RoleServer>, meta: &RequestMetaObject) -> String {
    meta.client_info()
        .map(|info| info.name)
        .or_else(|| client.peer_info().map(|info| info.client_info.name.clone()))
        .unwrap_or_else(|| "unknown agent".to_string())
}

impl Server {
    // Clients start servers anywhere (not always in the project), so the agent always names its directory.
    fn project_key(&self, vault: &Vault, dir: &std::path::Path, agent: &str) -> Option<String> {
        change::identify(&self.config, vault, dir, agent)
    }
}

// The whole Index when it fits the budget and no page is asked for; otherwise the page asked for, the first by default.
fn page_of(
    sections: &[index::Section],
    topics: Option<&str>,
    conflicts: &[String],
    head: &str,
    whole: String,
    page: Option<usize>,
    max: usize,
) -> Result<String, String> {
    if page.is_none() && whole.len() <= max {
        return Ok(whole);
    }
    let pages = index::pages(sections, topics, conflicts, head, max);
    let number = page.unwrap_or(1);
    let total = pages.len();
    number
        .checked_sub(1)
        .and_then(|i| pages.into_iter().nth(i))
        .ok_or_else(|| {
            format!("page {number} does not exist: this Index has {total} pages, from 1")
        })
}

#[tool_router]
impl Server {
    #[tool(
        description = "The memory rules and Index: preferences, the current project's notes and the topic names, one line each; with `topic`, that topic's notes. Call it first in every task (it returns the memory rules and the Index) unless an \"Agent memory (petit-poucet)\" block is already in your context."
    )]
    fn memory_index(
        &self,
        Parameters(req): Parameters<IndexRequest>,
        client: Peer<RoleServer>,
        meta: RequestMetaObject,
    ) -> Result<String, String> {
        let agent = agent_name(&client, &meta);
        let vault = self.vault()?;
        let max = self.config.limits.session_start_max_bytes;
        if let Some(topic) = &req.topic {
            let topic = slug::slugify(topic);
            let sections = index::sections(&vault, |place| place == Place::Topic(&topic));
            let whole = index::for_topic(&vault, &topic);
            let whole = match whole.trim() {
                "" => format!("no notes in topic {topic}"),
                view => view.to_string(),
            };
            return page_of(&sections, None, &[], "", whole, req.page, max);
        }
        let project = self.project_key(&vault, &req.project_dir, &agent);
        let project = project.as_deref();
        let view = index::for_project(&vault, project);
        let view = match view.trim() {
            "" => "no notes yet",
            view => view,
        };
        let conflicts = context::conflict_details(&vault);
        let whole = format!(
            "{}\n\n{view}{}",
            context::rules(false),
            index::conflicts_block(&conflicts)
        );
        let sections = index::sections(&vault, |place| index::in_session(place, project));
        let topics = index::topics_line(&vault);
        let head = format!("{}\n", context::rules(true));
        page_of(
            &sections,
            topics.as_deref(),
            &conflicts,
            &head,
            whole,
            req.page,
            max,
        )
    }

    #[tool(description = "Read one note, or a sync conflict copy of one.")]
    fn memory_read(&self, Parameters(req): Parameters<ReadRequest>) -> Result<String, String> {
        let vault = self.vault()?;
        if let Some(conflict) = vault.conflict(&req.path) {
            let text =
                fs::read_to_string(vault.root.join(&conflict.copy)).map_err(|e| e.to_string())?;
            self.reads.record(&conflict.copy, &text);
            return Ok(text);
        }
        let note = change::find_note(&vault, &req.path)?;
        let file = note_file(&note.path);
        let text = fs::read_to_string(vault.root.join(&file)).map_err(|e| e.to_string())?;
        self.reads.record(&file, &text);
        let counted = self.lock().and_then(|_lock| {
            usage::record_read(&vault.root, &note.path, jiff::Zoned::now().date())
        });
        if let Err(e) = counted {
            eprintln!("petit-poucet: usage not recorded: {e}");
        }
        Ok(text)
    }

    #[tool(
        description = "For a memory cleanup review, in pages: the notes, one line each under their folder (every note in a small vault, else preferences, changed folders and recently used folders), then the problems found in the whole vault. Page 1 builds the review; the next pages come from it unchanged. Reading the last page marks the cleanup done; `done: true` marks it again once its fixes are applied. Only for the memory-cleanup agent and the tidy-memory skill."
    )]
    fn memory_review(
        &self,
        Parameters(req): Parameters<ReviewRequest>,
        client: Peer<RoleServer>,
        meta: RequestMetaObject,
    ) -> Result<String, String> {
        let agent = agent_name(&client, &meta);
        if req.done == Some(true) {
            cleanup::record(&self.config, SystemTime::now(), &agent)?;
            return Ok(
                "cleanup recorded: its fixes won't count towards the next reminder".to_string(),
            );
        }
        self.review.page(
            &self.config,
            req.page.unwrap_or(1),
            req.full.unwrap_or(false),
            &agent,
        )
    }

    #[tool(
        description = "Create or update one short fact: needs type, title, summary, fact, source and how_to_apply. Search first; read a note before updating it; updating a `feedback` note needs the user's confirmation."
    )]
    fn memory_save(
        &self,
        Parameters(req): Parameters<SaveRequest>,
        client: Peer<RoleServer>,
        meta: RequestMetaObject,
    ) -> Result<String, String> {
        let agent = agent_name(&client, &meta);
        let _lock = self.lock()?;
        if let Some(name) = &req.path {
            let path = self.resolve(name)?;
            self.reads.check(&self.config.vault, &note_file(&path))?;
        }
        let (path, reply) = save::save(&self.config, req, &agent)?;
        self.reads
            .record_file(&self.config.vault, &note_file(&path));
        Ok(reply)
    }

    #[tool(
        description = "Delete a wrong or obsolete note and unlink it everywhere, or a sync conflict copy once merged. Deleting a `feedback` note needs the user's confirmation."
    )]
    fn memory_delete(
        &self,
        Parameters(req): Parameters<DeleteRequest>,
        client: Peer<RoleServer>,
        meta: RequestMetaObject,
    ) -> Result<String, String> {
        let agent = agent_name(&client, &meta);
        let _lock = self.lock()?;
        if let Some(conflict) = self.vault()?.conflict(&req.path) {
            self.reads
                .check_delete(&self.config.vault, &conflict.copy)?;
            let reply = delete::delete_copy(&self.config.vault, conflict)?;
            self.reads.forget(&conflict.copy);
            return Ok(reply);
        }
        let path = self.resolve(&req.path)?;
        self.reads
            .check_delete(&self.config.vault, &note_file(&path))?;
        let (reply, rewritten) = delete::delete(&self.config, req, &agent)?;
        self.reads.forget(&note_file(&path));
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
        meta: RequestMetaObject,
    ) -> Result<String, String> {
        let agent = agent_name(&client, &meta);
        let _lock = self.lock()?;
        let path = self.resolve(&req.path)?;
        let new_path = req.new_path.trim_end_matches(".md").to_string();
        let (reply, rewritten) = move_note::move_note(&self.config, req, &agent)?;
        self.reads.forget(&note_file(&path));
        for file in &rewritten {
            self.reads.refresh(&self.config.vault, file);
        }
        usage::relocate(&self.config.vault, &path, Some(&new_path))?;
        Ok(reply)
    }

    #[tool(
        description = "Word search over titles, summaries and bodies of the preferences, the current project and all topics (all notes when the project is unknown); best matches first."
    )]
    fn memory_search(
        &self,
        Parameters(req): Parameters<SearchRequest>,
        client: Peer<RoleServer>,
        meta: RequestMetaObject,
    ) -> Result<String, String> {
        let agent = agent_name(&client, &meta);
        let vault = self.vault()?;
        let project = self.project_key(&vault, &req.project_dir, &agent);
        // An unknown project would hide every project note: search them all rather than miss the right one.
        let notes = vault.notes.iter().filter(|n| {
            project.is_none()
                || index::in_session(n.place(), project.as_deref())
                || matches!(n.place(), Place::Topic(_))
        });
        let found = search::search(notes, &req.query);
        if found.is_empty() {
            return Ok("no matches".to_string());
        }
        let lines: Vec<String> = found
            .iter()
            .map(|n| {
                format!(
                    "- [[{}]] — {}{}",
                    n.path,
                    n.summary().unwrap_or(index::NO_SUMMARY),
                    index::when_suffix(n)
                )
            })
            .collect();
        Ok(lines.join("\n"))
    }
}

#[tool_handler]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(INSTRUCTIONS)
    }
}

pub fn serve(config: Config) -> Result<(), String> {
    Vault::load(&config.vault)?;
    let server = Server {
        config,
        reads: ReadLog::default(),
        review: ReviewSession::default(),
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

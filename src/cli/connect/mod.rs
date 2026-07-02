//! LIF-249: `lific connect` — write MCP config into AI clients from the CLI.
//!
//! The flagship onboarding command. It replaces the copy-a-snippet web page as
//! the primary path: pick clients (interactively or via `--client`), mint (or
//! reuse) an API key, and write correct MCP config directly into each client's
//! native config file — merging non-destructively.
//!
//! Structure:
//! - [`clients`] — the canonical server config + per-client schema/path matrix.
//! - [`writer`]  — format-native, merge-preserving JSON/TOML/YAML writers.
//! - this module — orchestration: CLI args → detection → key minting → writes →
//!   output, plus the optional AGENTS.md step (LIF-251).
//!
//! ## Key-minting & authz semantics (investigated, LIF-249)
//!
//! An API key with `user_id = NULL` (an "unassigned" key) behaves very
//! differently under the two authz modes (see `src/authz.rs`):
//! - **Legacy mode (default, `authz_enforced = false`):** an unassigned key
//!   resolves to `AuthUser = None`, which `require_role` passes unconditionally
//!   at Viewer/Maintainer. It can read and write everything — exactly like
//!   `lific start`'s first-run "default" key. Fine for a local single-user box.
//! - **Enforced mode (`authz_enforced = true`):** `None` is default-denied at
//!   every level and `visible_project_ids` returns the empty set — an
//!   unassigned key would **see nothing**. Shipping one there is a setup bug.
//!
//! Therefore `connect` prefers a **bot identity owned by a human** (parity with
//! the web UI's Connected Tools): the bot inherits its owner's role, so it works
//! under both modes. It only falls back to a plain unassigned key on a truly
//! fresh install (zero human users) — where enforcement can't be on yet (it
//! takes an admin to enable). If humans exist but none can be chosen
//! unambiguously and no `--user` was given, we surface guidance rather than mint
//! a key that might see nothing.
//!
//! One key is minted per `connect` run and shared by all clients selected in
//! that run (documented choice; the web UI mints per-tool, which we can revisit).

pub mod clients;
pub mod writer;

use std::io::IsTerminal;
use std::path::PathBuf;

use crate::config::Config;
use crate::db::DbPool;

use clients::{Os, PathBase, Scope, ServerConfig, Transport};

/// Parsed, validated arguments for a `connect` run. Built from the CLI enum in
/// `cli/mod.rs` so the heavy lifting here is testable without clap.
pub struct ConnectArgs {
    pub clients: Vec<String>,
    pub scope: Scope,
    pub stdio: bool,
    pub url: Option<String>,
    pub key: Option<String>,
    pub user: Option<String>,
    pub yes: bool,
    pub dry_run: bool,
    pub skip_agents: bool,
}

/// The outcome for a single client write, for both human and JSON output.
#[derive(Debug)]
pub struct ClientOutcome {
    pub id: String,
    pub display: String,
    pub format: String,
    pub path: Option<PathBuf>,
    pub action: Option<String>,
    pub notes: Vec<String>,
    pub error: Option<String>,
    pub manual_snippet: Option<String>,
    /// The full file body, for `--dry-run` display.
    pub dry_run_contents: Option<String>,
}

/// How the shared key was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOrigin {
    /// Supplied by the user via `--key`.
    Provided,
    /// Minted as a bot identity owned by a human user.
    Bot,
    /// Minted as a plain unassigned key (fresh install, zero users).
    Unassigned,
}

/// The full result of a run, returned so `main` (and tests) can render it.
pub struct ConnectResult {
    pub outcomes: Vec<ClientOutcome>,
    pub key: Option<String>,
    pub key_origin: Option<KeyOrigin>,
    pub key_name: Option<String>,
    pub agents_md: Option<AgentsMdOutcome>,
    pub dry_run: bool,
    pub stdio: bool,
    pub url: String,
}

#[derive(Debug)]
pub struct AgentsMdOutcome {
    pub path: PathBuf,
    pub action: String,
}

/// Build the production [`PathBase`] from the real environment.
///
/// A `LIFIC_CONNECT_HOME` override is honored for the home dir. It exists for
/// smoke-testing so a manual run can be pointed at a scratch dir instead of the
/// operator's real `~/.config` — documented as test-only.
pub fn production_base() -> Result<PathBase, String> {
    let home = std::env::var_os("LIFIC_CONNECT_HOME")
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .ok_or_else(|| "could not determine home directory".to_string())?;
    let project = std::env::current_dir().map_err(|e| format!("cannot read cwd: {e}"))?;
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    Ok(PathBase {
        home,
        project,
        os: Os::host(),
        appdata,
    })
}

/// Compute the default MCP URL for remote configs.
///
/// Prefer `server.public_url` (with `/mcp` appended if it isn't already there).
/// Otherwise `http://127.0.0.1:{port}/mcp` — never `0.0.0.0`, which is a bind
/// address, not something a client can dial.
pub fn default_url(cfg: &Config) -> String {
    if let Some(pu) = cfg.server.public_url.as_deref() {
        let trimmed = pu.trim().trim_end_matches('/');
        if trimmed.ends_with("/mcp") {
            return trimmed.to_string();
        }
        return format!("{trimmed}/mcp");
    }
    format!("http://127.0.0.1:{}/mcp", cfg.server.port)
}

/// Absolute DB path for stdio configs (canonicalized when the file exists, else
/// made absolute against cwd so the spawned server opens the right file).
pub fn absolute_db_path(cfg: &Config) -> String {
    let p = &cfg.database.path;
    if let Ok(canon) = std::fs::canonicalize(p) {
        return canon.display().to_string();
    }
    if p.is_absolute() {
        return p.display().to_string();
    }
    match std::env::current_dir() {
        Ok(cwd) => cwd.join(p).display().to_string(),
        Err(_) => p.display().to_string(),
    }
}

/// Build the canonical [`ServerConfig`] for this run.
fn build_server_config(args: &ConnectArgs, cfg: &Config, key: &str) -> ServerConfig {
    if args.stdio {
        ServerConfig::stdio(absolute_db_path(cfg))
    } else {
        let url = args.url.clone().unwrap_or_else(|| default_url(cfg));
        ServerConfig::remote(url, key)
    }
}

// ── Client selection ─────────────────────────────────────────

/// Resolve the list of client ids to write. Explicit `--client` wins (each is
/// validated). With none given and a TTY, run the interactive picker. With none
/// and no TTY, refuse — naming the flags a non-interactive caller must pass.
///
/// Factored to take an injected `stdin_tty` and a picker closure so the refusal
/// branch is unit-testable (mirrors `term::confirm_inner`).
pub fn resolve_clients_inner(
    requested: &[String],
    stdin_tty: bool,
    base: &PathBase,
    scope: Scope,
    picker: impl FnOnce(&[DetectedClient]) -> Result<Vec<String>, String>,
) -> Result<Vec<String>, String> {
    if !requested.is_empty() {
        for id in requested {
            if clients::find_client(id).is_none() {
                return Err(format!(
                    "unknown client '{id}'. Known clients: {}",
                    clients::all_client_ids().join(", ")
                ));
            }
        }
        // De-dup while preserving order.
        let mut seen = std::collections::HashSet::new();
        return Ok(requested
            .iter()
            .filter(|id| seen.insert((*id).clone()))
            .cloned()
            .collect());
    }

    if !stdin_tty {
        return Err(
            "no client selected and stdin is not a TTY. Pass --client <id> (repeatable) to choose \
             clients, and --yes to skip prompts. Run with --client to see the list."
                .into(),
        );
    }

    let detected = detect_clients(base, scope);
    picker(&detected)
}

/// A client and whether it was detected in the given scope.
#[derive(Debug, Clone)]
pub struct DetectedClient {
    pub id: String,
    pub display: String,
    pub detected: bool,
}

/// Probe the filesystem for every client's config presence in `scope`.
pub fn detect_clients(base: &PathBase, scope: Scope) -> Vec<DetectedClient> {
    clients::all_clients()
        .iter()
        .map(|c| DetectedClient {
            id: c.id.to_string(),
            display: c.display.to_string(),
            detected: c.detected(base, scope),
        })
        .collect()
}

/// The default interactive picker: print detected clients numbered, read a
/// comma-separated selection (or "all"), return the chosen ids.
fn interactive_picker(detected: &[DetectedClient]) -> Result<Vec<String>, String> {
    use std::io::Write;

    let any_installed = detected.iter().any(|c| c.detected);
    let list: Vec<&DetectedClient> = if any_installed {
        detected.iter().filter(|c| c.detected).collect()
    } else {
        detected.iter().collect()
    };

    let mut err = std::io::stderr();
    if !any_installed {
        let _ = writeln!(
            err,
            "No installed clients detected in this scope. All known clients:"
        );
    } else {
        let _ = writeln!(err, "Detected clients:");
    }
    for (i, c) in list.iter().enumerate() {
        let _ = writeln!(err, "  {}. {} ({})", i + 1, c.display, c.id);
    }
    let _ = write!(
        err,
        "Select clients to configure (comma-separated numbers, or 'all'): "
    );
    let _ = err.flush();

    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| format!("failed to read selection: {e}"))?;
    let line = line.trim();
    if line.is_empty() {
        return Err("no selection made".into());
    }
    if line.eq_ignore_ascii_case("all") {
        return Ok(list.iter().map(|c| c.id.clone()).collect());
    }
    let mut chosen = Vec::new();
    for tok in line.split(',') {
        let tok = tok.trim();
        if tok.is_empty() {
            continue;
        }
        let n: usize = tok
            .parse()
            .map_err(|_| format!("invalid selection '{tok}'"))?;
        if n == 0 || n > list.len() {
            return Err(format!("selection {n} out of range"));
        }
        chosen.push(list[n - 1].id.clone());
    }
    if chosen.is_empty() {
        return Err("no valid selection".into());
    }
    Ok(chosen)
}

// ── Key minting ──────────────────────────────────────────────

/// Decide how to obtain the API key for this run and produce it.
///
/// Returns `(key, origin, key_name)`. See the module docs for the authz
/// rationale behind preferring a bot identity.
fn obtain_key(
    args: &ConnectArgs,
    pool: &DbPool,
    key_name: &str,
) -> Result<(String, KeyOrigin, String), String> {
    if let Some(k) = &args.key {
        return Ok((k.clone(), KeyOrigin::Provided, "(provided)".into()));
    }

    let manager = crate::auth::create_key_manager()
        .map_err(|e| format!("key manager init failed: {e}"))?;

    // Choose an owner: explicit --user, else the sole non-bot user if there's
    // exactly one, else (with multiple humans) require --user.
    let owner = choose_owner(pool, args.user.as_deref())?;

    match owner {
        OwnerChoice::User(user_id) => {
            // Mint a bot owned by this user, mirroring the web UI's Connected
            // Tools so the key inherits the owner's role under authz enforcement.
            let bot_username = key_name.to_string();
            // Reuse an existing bot of this name if present, else create one
            // owned by the chosen human — exactly the web UI's Connected Tools
            // shape. The API key is assigned to the BOT, whose owner_id points
            // at the human, so authz resolves bot → owner (src/authz.rs).
            let bot_id = {
                let conn = pool.write().map_err(|e| e.to_string())?;
                match crate::db::queries::users::find_bot_by_username(&conn, &bot_username)
                    .map_err(|e| e.to_string())?
                {
                    Some(existing) => existing.id,
                    None => crate::db::queries::users::create_bot_user(
                        &conn,
                        user_id,
                        &bot_username,
                        "Lific Connect",
                    )
                    .map_err(|e| e.to_string())?
                    .id,
                }
            };
            let key = mint_or_rotate(pool, &manager, &bot_username)?;
            {
                let conn = pool.write().map_err(|e| e.to_string())?;
                crate::db::queries::users::assign_key_to_user(&conn, &bot_username, bot_id)
                    .map_err(|e| e.to_string())?;
            }
            Ok((key, KeyOrigin::Bot, bot_username))
        }
        OwnerChoice::FreshInstall => {
            // Zero human users: enforcement can't be on (needs an admin to
            // enable), so a plain unassigned key behaves like `lific start`'s
            // first-run default key. Safe here, and the only workable option.
            let key = mint_or_rotate(pool, &manager, key_name)?;
            Ok((key, KeyOrigin::Unassigned, key_name.to_string()))
        }
    }
}

/// Create a key named `name`, or — if an active key with that name already
/// exists (a previous `connect` run) — rotate it instead so re-running
/// `connect` (e.g. to add another client later) always succeeds with a fresh
/// plaintext. Rotation preserves any existing user binding.
fn mint_or_rotate(
    pool: &DbPool,
    manager: &api_keys_simplified::ApiKeyManagerV0,
    name: &str,
) -> Result<String, String> {
    let active_exists = {
        let conn = pool.read().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT COUNT(*) > 0 FROM api_keys WHERE name = ?1 AND revoked = 0",
            rusqlite::params![name],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false)
    };
    if active_exists {
        crate::auth::rotate_api_key(pool, manager, name).map_err(|e| e.to_string())
    } else {
        crate::auth::create_api_key(pool, manager, name).map_err(|e| e.to_string())
    }
}

enum OwnerChoice {
    User(i64),
    FreshInstall,
}

fn choose_owner(pool: &DbPool, requested_user: Option<&str>) -> Result<OwnerChoice, String> {
    let conn = pool.read().map_err(|e| e.to_string())?;

    if let Some(username) = requested_user {
        let u = crate::db::queries::users::get_user_by_username(&conn, username)
            .map_err(|_| format!("user '{username}' not found"))?;
        return Ok(OwnerChoice::User(u.id));
    }

    let users = crate::db::queries::users::list_users(&conn).map_err(|e| e.to_string())?;
    let humans: Vec<_> = users.iter().filter(|u| !u.is_bot).collect();

    match humans.len() {
        0 => Ok(OwnerChoice::FreshInstall),
        1 => Ok(OwnerChoice::User(humans[0].id)),
        _ => {
            // Prefer a single admin if there's exactly one; otherwise require
            // an explicit choice rather than guessing (and risk a key that sees
            // nothing under enforcement, or is owned by the wrong person).
            let admins: Vec<_> = humans.iter().filter(|u| u.is_admin).collect();
            if admins.len() == 1 {
                Ok(OwnerChoice::User(admins[0].id))
            } else {
                Err(
                    "multiple users exist — pass --user <username> to choose which user owns the \
                     connection's API key (it inherits that user's project access)."
                        .into(),
                )
            }
        }
    }
}

/// A stable-ish name for the minted key/bot: `connect-<host>` where host comes
/// from HOSTNAME/COMPUTERNAME, falling back to `cli`.
fn key_name_for_run() -> String {
    let host = std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty());
    match host {
        Some(h) => {
            // Sanitize to a safe token.
            let safe: String = h
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect();
            format!("connect-{safe}")
        }
        None => "connect-cli".to_string(),
    }
}

// ── The run ──────────────────────────────────────────────────

/// Execute a `connect` run: select clients, mint/reuse a key, render or write
/// each client's config, and (optionally) update AGENTS.md. Pure enough to test
/// end-to-end against a temp base + in-memory DB.
pub fn run(
    args: &ConnectArgs,
    cfg: &Config,
    pool: &DbPool,
    base: &PathBase,
) -> Result<ConnectResult, String> {
    let stdin_tty = std::io::stdin().is_terminal();
    let selected = resolve_clients_inner(
        &args.clients,
        stdin_tty,
        base,
        args.scope,
        interactive_picker,
    )?;

    // Obtain the key up front (unless stdio, which needs no key at all).
    let key_name = key_name_for_run();
    let (key, key_origin) = if args.stdio {
        (String::new(), None)
    } else if args.dry_run && args.key.is_none() {
        // Don't mint a real key just to preview; use a clearly-fake placeholder.
        (
            "lific_sk-live-DRYRUN000000000000000000000000".to_string(),
            Some(KeyOrigin::Provided),
        )
    } else {
        let (k, o, _name) = obtain_key(args, pool, &key_name)?;
        (k, Some(o))
    };

    let server = build_server_config(args, cfg, &key);
    let outcomes = write_all_clients(&selected, &server, base, args.scope, args.dry_run);

    // AGENTS.md (LIF-251).
    let agents_md = maybe_write_agents_md(args, base, stdin_tty)?;

    let key_out = match &server.transport {
        Transport::Stdio { .. } => None,
        Transport::Remote { .. } => Some(key.clone()),
    };
    let key_name_out = match key_origin {
        Some(KeyOrigin::Provided) | None => None,
        _ => Some(key_name.clone()),
    };

    Ok(ConnectResult {
        outcomes,
        key: key_out,
        key_origin,
        key_name: key_name_out,
        agents_md,
        dry_run: args.dry_run,
        stdio: args.stdio,
        url: match &server.transport {
            Transport::Remote { url, .. } => url.clone(),
            Transport::Stdio { db_path } => db_path.clone(),
        },
    })
}

fn write_all_clients(
    selected: &[String],
    server: &ServerConfig,
    base: &PathBase,
    scope: Scope,
    dry_run: bool,
) -> Vec<ClientOutcome> {
    let mut outcomes = Vec::new();
    for id in selected {
        let Some(spec) = clients::find_client(id) else {
            continue;
        };
        let Some(path) = spec.path_for(base, scope) else {
            outcomes.push(ClientOutcome {
                id: id.clone(),
                display: spec.display.to_string(),
                format: spec.format.as_str().to_string(),
                path: None,
                action: None,
                notes: vec![],
                error: Some(format!(
                    "{} has no {}-scope config; skipped",
                    spec.display,
                    scope.as_str()
                )),
                manual_snippet: None,
                dry_run_contents: None,
            });
            continue;
        };

        let entry = spec.compile(server);
        if dry_run {
            match writer::render(&path, spec.format, &entry) {
                Ok(rendered) => outcomes.push(ClientOutcome {
                    id: id.clone(),
                    display: spec.display.to_string(),
                    format: spec.format.as_str().to_string(),
                    path: Some(path),
                    action: Some(rendered.action.as_str().to_string()),
                    notes: entry.notes.clone(),
                    error: None,
                    manual_snippet: None,
                    dry_run_contents: Some(rendered.contents),
                }),
                Err(e) => outcomes.push(ClientOutcome {
                    id: id.clone(),
                    display: spec.display.to_string(),
                    format: spec.format.as_str().to_string(),
                    path: Some(path),
                    action: None,
                    notes: entry.notes.clone(),
                    error: Some(e.message.clone()),
                    manual_snippet: e.manual_snippet,
                    dry_run_contents: None,
                }),
            }
        } else {
            match writer::write(&path, spec.format, &entry) {
                Ok(action) => outcomes.push(ClientOutcome {
                    id: id.clone(),
                    display: spec.display.to_string(),
                    format: spec.format.as_str().to_string(),
                    path: Some(path),
                    action: Some(action.as_str().to_string()),
                    notes: entry.notes.clone(),
                    error: None,
                    manual_snippet: None,
                    dry_run_contents: None,
                }),
                Err(e) => outcomes.push(ClientOutcome {
                    id: id.clone(),
                    display: spec.display.to_string(),
                    format: spec.format.as_str().to_string(),
                    path: Some(path),
                    action: None,
                    notes: entry.notes.clone(),
                    error: Some(e.message.clone()),
                    manual_snippet: e.manual_snippet,
                    dry_run_contents: None,
                }),
            }
        }
    }
    outcomes
}

/// Decide whether and how to touch AGENTS.md for this run.
///
/// Only in project scope, or when cwd looks like a project (has `.git`).
/// `--skip-agents` opts out silently. In interactive mode we'd ask; here the
/// consent model is: with `--yes` (or `--skip-agents`) the decision is explicit,
/// so in project scope with `--yes` we write it. Without a TTY and without
/// `--yes`, we skip (don't hang, don't surprise-write).
fn maybe_write_agents_md(
    args: &ConnectArgs,
    base: &PathBase,
    stdin_tty: bool,
) -> Result<Option<AgentsMdOutcome>, String> {
    if args.skip_agents {
        return Ok(None);
    }
    if args.dry_run {
        return Ok(None);
    }

    let looks_like_project =
        args.scope == Scope::Project || base.project.join(".git").exists();
    if !looks_like_project {
        return Ok(None);
    }

    // Consent: explicit --yes writes; interactive TTY asks; otherwise skip.
    let consented = if args.yes {
        true
    } else if stdin_tty {
        crate::cli::term::confirm(
            "Write a Lific block into ./AGENTS.md so agents in this repo know about it?",
            "--yes",
        )
        .unwrap_or(false)
    } else {
        false
    };
    if !consented {
        return Ok(None);
    }

    let path = base.project.join("AGENTS.md");
    let action = crate::cli::agents_md::write(&path, None)
        .map_err(|e| format!("AGENTS.md update failed: {e}"))?;
    Ok(Some(AgentsMdOutcome {
        path,
        action: action.as_str().to_string(),
    }))
}

// ── Output rendering ─────────────────────────────────────────

/// Render a run result to stdout, honoring `json`.
pub fn print_result(result: &ConnectResult, json: bool) {
    if json {
        print_json(result);
    } else {
        print_human(result);
    }
}

fn print_json(result: &ConnectResult) {
    let clients: Vec<serde_json::Value> = result
        .outcomes
        .iter()
        .map(|o| {
            serde_json::json!({
                "id": o.id,
                "format": o.format,
                "path": o.path.as_ref().map(|p| p.display().to_string()),
                "action": o.action,
                "notes": o.notes,
                "error": o.error,
                "manual_snippet": o.manual_snippet,
                "contents": o.dry_run_contents,
            })
        })
        .collect();
    let out = serde_json::json!({
        "clients": clients,
        "key": result.key,
        "key_name": result.key_name,
        "dry_run": result.dry_run,
        "stdio": result.stdio,
        "url": result.url,
        "agents_md": result.agents_md.as_ref().map(|a| serde_json::json!({
            "path": a.path.display().to_string(),
            "action": a.action,
        })),
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

fn print_human(result: &ConnectResult) {
    println!();
    if result.dry_run {
        println!("  Dry run — no files were written.");
        println!();
    }
    for o in &result.outcomes {
        match (&o.action, &o.error) {
            (Some(action), _) => {
                let path = o
                    .path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                println!("  [{}] {action}: {path}", o.display);
            }
            (None, Some(err)) => {
                println!("  [{}] skipped: {err}", o.display);
                if let Some(snippet) = &o.manual_snippet {
                    println!("      Merge this in manually:");
                    for line in snippet.lines() {
                        println!("        {line}");
                    }
                }
            }
            (None, None) => {}
        }
        for note in &o.notes {
            println!("      note: {note}");
        }
        if result.dry_run
            && let Some(contents) = &o.dry_run_contents
        {
            println!("      ---");
            for line in contents.lines() {
                println!("      {line}");
            }
            println!("      ---");
        }
    }

    if let Some(a) = &result.agents_md {
        println!();
        println!("  AGENTS.md {}: {}", a.action, a.path.display());
    }

    if let Some(key) = &result.key {
        println!();
        println!("  API key for this connection:");
        println!();
        println!("    {key}");
        println!();
        match result.key_origin {
            Some(KeyOrigin::Provided) => {}
            _ => {
                println!("  Save this key now. It will never be shown again.");
            }
        }
        if let Some(KeyOrigin::Unassigned) = result.key_origin {
            println!(
                "  (Unassigned key — full access on this local instance. Create a user and \
                 re-run --user <name> if you enable project authorization.)"
            );
        }
        // Codex reads the key from an env var — print the export hint if codex
        // was among the selected clients.
        if result
            .outcomes
            .iter()
            .any(|o| o.id == "codex" && o.error.is_none())
        {
            println!();
            println!("  For Codex, export the key before launching it:");
            println!("    export LIFIC_API_KEY=\"{key}\"");
        }
    }

    println!();
    println!("  Restart your client(s) to pick up the new MCP server.");
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn cfg_with_port(port: u16) -> Config {
        let mut c = Config::default();
        c.server.port = port;
        c
    }

    fn base(dir: &std::path::Path) -> PathBase {
        PathBase {
            home: dir.join("home"),
            project: dir.join("proj"),
            os: Os::Linux,
            appdata: None,
        }
    }

    fn tmp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "lific-connect-run-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    // ── default_url ──────────────────────────────────────────

    #[test]
    fn default_url_uses_loopback_and_port_not_bind_host() {
        let c = cfg_with_port(9999);
        assert_eq!(default_url(&c), "http://127.0.0.1:9999/mcp");
    }

    #[test]
    fn default_url_prefers_public_url_and_appends_mcp() {
        let mut c = Config::default();
        c.server.public_url = Some("https://lific.example.com".into());
        assert_eq!(default_url(&c), "https://lific.example.com/mcp");
    }

    #[test]
    fn default_url_public_url_already_has_mcp() {
        let mut c = Config::default();
        c.server.public_url = Some("https://lific.example.com/mcp".into());
        assert_eq!(default_url(&c), "https://lific.example.com/mcp");
    }

    // ── resolve_clients_inner ────────────────────────────────

    fn no_picker(_: &[DetectedClient]) -> Result<Vec<String>, String> {
        panic!("picker must not be called when --client is given or stdin is not a TTY");
    }

    #[test]
    fn resolve_explicit_clients_validates_and_dedups() {
        let dir = tmp();
        let b = base(&dir);
        let got = resolve_clients_inner(
            &["opencode".into(), "codex".into(), "opencode".into()],
            true,
            &b,
            Scope::Global,
            no_picker,
        )
        .unwrap();
        assert_eq!(got, vec!["opencode".to_string(), "codex".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_unknown_client_errors() {
        let dir = tmp();
        let b = base(&dir);
        let err = resolve_clients_inner(
            &["nope".into()],
            true,
            &b,
            Scope::Global,
            no_picker,
        )
        .unwrap_err();
        assert!(err.contains("unknown client"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_no_client_non_tty_refuses_naming_flags() {
        let dir = tmp();
        let b = base(&dir);
        let err = resolve_clients_inner(&[], false, &b, Scope::Global, no_picker).unwrap_err();
        assert!(err.contains("--client"), "must name --client: {err}");
        assert!(err.contains("--yes"), "must name --yes: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_no_client_tty_calls_picker() {
        let dir = tmp();
        let b = base(&dir);
        let got = resolve_clients_inner(&[], true, &b, Scope::Global, |_| {
            Ok(vec!["cursor".into()])
        })
        .unwrap();
        assert_eq!(got, vec!["cursor".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }

    // ── detection ────────────────────────────────────────────

    #[test]
    fn detect_finds_only_present_clients() {
        let dir = tmp();
        let b = base(&dir);
        // Create ~/.cursor/ and ~/.codex/config.toml in the injected home.
        std::fs::create_dir_all(b.home.join(".cursor")).unwrap();
        std::fs::create_dir_all(b.home.join(".codex")).unwrap();
        std::fs::write(b.home.join(".codex").join("config.toml"), "").unwrap();

        let detected = detect_clients(&b, Scope::Global);
        let by_id = |id: &str| detected.iter().find(|c| c.id == id).unwrap().detected;
        assert!(by_id("cursor"), "cursor should be detected");
        assert!(by_id("codex"), "codex should be detected");
        assert!(!by_id("gemini"), "gemini should not be detected");
        assert!(!by_id("windsurf"), "windsurf should not be detected");
        std::fs::remove_dir_all(&dir).ok();
    }

    // ── end-to-end run ───────────────────────────────────────

    fn args(clients: &[&str], scope: Scope) -> ConnectArgs {
        ConnectArgs {
            clients: clients.iter().map(|s| s.to_string()).collect(),
            scope,
            stdio: false,
            url: Some("http://127.0.0.1:3456/mcp".into()),
            key: Some("lific_sk-live-TESTKEY".into()),
            user: None,
            yes: true,
            dry_run: false,
            skip_agents: true,
        }
    }

    #[test]
    fn run_writes_project_scope_configs_and_skips_no_project_clients() {
        let dir = tmp();
        let b = base(&dir);
        let pool = db::open_memory().unwrap();
        let cfg = Config::default();
        // goose has no project path → should be skipped with a warning.
        let a = args(&["opencode", "codex", "goose"], Scope::Project);
        let result = run(&a, &cfg, &pool, &b).unwrap();

        let oc = result.outcomes.iter().find(|o| o.id == "opencode").unwrap();
        assert_eq!(oc.action.as_deref(), Some("created"));
        assert!(b.project.join("opencode.json").exists());

        let cx = result.outcomes.iter().find(|o| o.id == "codex").unwrap();
        assert_eq!(cx.action.as_deref(), Some("created"));
        assert!(b.project.join(".codex/config.toml").exists());

        let goose = result.outcomes.iter().find(|o| o.id == "goose").unwrap();
        assert!(goose.action.is_none());
        assert!(
            goose.error.as_ref().unwrap().contains("project"),
            "goose skip should mention project scope"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn run_stdio_writes_absolute_db_and_no_key() {
        let dir = tmp();
        let b = base(&dir);
        let pool = db::open_memory().unwrap();
        let mut cfg = Config::default();
        cfg.database.path = dir.join("mydb.db");
        let mut a = args(&["opencode"], Scope::Project);
        a.stdio = true;
        a.key = None;

        let result = run(&a, &cfg, &pool, &b).unwrap();
        assert!(result.key.is_none(), "stdio needs no key");

        let written =
            std::fs::read_to_string(b.project.join("opencode.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&written).unwrap();
        assert_eq!(v["mcp"]["lific"]["type"], "local");
        let cmd = v["mcp"]["lific"]["command"].as_array().unwrap();
        // The db path is absolute.
        let db_arg = cmd[2].as_str().unwrap();
        assert!(
            std::path::Path::new(db_arg).is_absolute(),
            "stdio db path must be absolute, got {db_arg}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn run_dry_run_writes_nothing_but_returns_contents() {
        let dir = tmp();
        let b = base(&dir);
        let pool = db::open_memory().unwrap();
        let cfg = Config::default();
        let mut a = args(&["cursor"], Scope::Global);
        a.dry_run = true;

        let result = run(&a, &cfg, &pool, &b).unwrap();
        let oc = &result.outcomes[0];
        assert!(oc.dry_run_contents.is_some());
        // Nothing on disk.
        assert!(!b.home.join(".cursor/mcp.json").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    // ── key minting ──────────────────────────────────────────

    #[test]
    fn obtain_key_provided_is_verbatim() {
        let pool = db::open_memory().unwrap();
        let a = ConnectArgs {
            clients: vec![],
            scope: Scope::Global,
            stdio: false,
            url: None,
            key: Some("lific_sk-live-XYZ".into()),
            user: None,
            yes: true,
            dry_run: false,
            skip_agents: true,
        };
        let (k, o, _n) = obtain_key(&a, &pool, "connect-test").unwrap();
        assert_eq!(k, "lific_sk-live-XYZ");
        assert_eq!(o, KeyOrigin::Provided);
    }

    #[test]
    fn obtain_key_fresh_install_zero_users_mints_unassigned() {
        let pool = db::open_memory().unwrap();
        let a = ConnectArgs {
            clients: vec![],
            scope: Scope::Global,
            stdio: false,
            url: None,
            key: None,
            user: None,
            yes: true,
            dry_run: false,
            skip_agents: true,
        };
        let (k, o, name) = obtain_key(&a, &pool, "connect-test").unwrap();
        assert!(k.starts_with("lific_sk-live-"));
        assert_eq!(o, KeyOrigin::Unassigned);
        // The key is unassigned (user_id NULL).
        let conn = pool.read().unwrap();
        let uid: Option<i64> = conn
            .query_row(
                "SELECT user_id FROM api_keys WHERE name = ?1",
                rusqlite::params![name],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(uid, None);
    }

    #[test]
    fn obtain_key_single_user_mints_bot_owned_by_them() {
        let pool = db::open_memory().unwrap();
        let owner_id = {
            let conn = pool.write().unwrap();
            crate::db::queries::users::create_user(
                &conn,
                &crate::db::models::CreateUser {
                    username: "solo".into(),
                    email: "solo@test.com".into(),
                    password: "testpassword1".into(),
                    display_name: None,
                    is_admin: true,
                    is_bot: false,
                },
            )
            .unwrap()
            .id
        };

        let a = ConnectArgs {
            clients: vec![],
            scope: Scope::Global,
            stdio: false,
            url: None,
            key: None,
            user: None,
            yes: true,
            dry_run: false,
            skip_agents: true,
        };
        let (k, o, name) = obtain_key(&a, &pool, "connect-host").unwrap();
        assert!(k.starts_with("lific_sk-live-"));
        assert_eq!(o, KeyOrigin::Bot);

        let conn = pool.read().unwrap();
        // A bot user was created, owned by the human, and is a bot.
        let (is_bot, owner): (bool, Option<i64>) = conn
            .query_row(
                "SELECT is_bot, owner_id FROM users WHERE username = ?1",
                rusqlite::params![name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(is_bot, "connect must create a bot identity");
        assert_eq!(owner, Some(owner_id), "bot must be owned by the human");
        // The key is assigned to the bot.
        let key_owner: Option<i64> = conn
            .query_row(
                "SELECT u.owner_id FROM api_keys k JOIN users u ON u.id = k.user_id WHERE k.name = ?1",
                rusqlite::params![name],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(key_owner, Some(owner_id));

        // Re-running connect (e.g. adding another client later) must succeed:
        // the active key name is rotated, not rejected as a duplicate, and the
        // fresh key keeps the bot binding.
        let (k2, o2, name2) = obtain_key(&a, &pool, "connect-host").unwrap();
        assert!(k2.starts_with("lific_sk-live-"));
        assert_ne!(k2, k, "rotation must issue a fresh plaintext");
        assert_eq!(o2, KeyOrigin::Bot);
        assert_eq!(name2, name);
        let active_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM api_keys WHERE name = ?1 AND revoked = 0",
                rusqlite::params![name],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(active_count, 1, "exactly one active key after re-run");
    }

    #[test]
    fn obtain_key_explicit_user_is_honored() {
        let pool = db::open_memory().unwrap();
        let (alice, _bob) = {
            let conn = pool.write().unwrap();
            let alice = crate::db::queries::users::create_user(
                &conn,
                &crate::db::models::CreateUser {
                    username: "alice".into(),
                    email: "alice@test.com".into(),
                    password: "testpassword1".into(),
                    display_name: None,
                    is_admin: false,
                    is_bot: false,
                },
            )
            .unwrap()
            .id;
            let bob = crate::db::queries::users::create_user(
                &conn,
                &crate::db::models::CreateUser {
                    username: "bob".into(),
                    email: "bob@test.com".into(),
                    password: "testpassword1".into(),
                    display_name: None,
                    is_admin: false,
                    is_bot: false,
                },
            )
            .unwrap()
            .id;
            (alice, bob)
        };

        let a = ConnectArgs {
            clients: vec![],
            scope: Scope::Global,
            stdio: false,
            url: None,
            key: None,
            user: Some("alice".into()),
            yes: true,
            dry_run: false,
            skip_agents: true,
        };
        let (_k, o, name) = obtain_key(&a, &pool, "connect-host").unwrap();
        assert_eq!(o, KeyOrigin::Bot);
        let conn = pool.read().unwrap();
        let owner: Option<i64> = conn
            .query_row(
                "SELECT owner_id FROM users WHERE username = ?1",
                rusqlite::params![name],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(owner, Some(alice), "explicit --user must own the bot");
    }

    #[test]
    fn obtain_key_multiple_users_no_user_flag_errors_with_guidance() {
        let pool = db::open_memory().unwrap();
        {
            let conn = pool.write().unwrap();
            for (n, admin) in [("a", false), ("b", false)] {
                crate::db::queries::users::create_user(
                    &conn,
                    &crate::db::models::CreateUser {
                        username: n.into(),
                        email: format!("{n}@test.com"),
                        password: "testpassword1".into(),
                        display_name: None,
                        is_admin: admin,
                        is_bot: false,
                    },
                )
                .unwrap();
            }
        }
        let a = ConnectArgs {
            clients: vec![],
            scope: Scope::Global,
            stdio: false,
            url: None,
            key: None,
            user: None,
            yes: true,
            dry_run: false,
            skip_agents: true,
        };
        let err = obtain_key(&a, &pool, "connect-host").unwrap_err();
        assert!(err.contains("--user"), "must guide toward --user: {err}");
    }

    // ── AGENTS.md integration ────────────────────────────────

    #[test]
    fn run_writes_agents_md_in_project_scope_when_yes() {
        let dir = tmp();
        let b = base(&dir);
        std::fs::create_dir_all(&b.project).unwrap();
        let pool = db::open_memory().unwrap();
        let cfg = Config::default();
        let mut a = args(&["opencode"], Scope::Project);
        a.skip_agents = false; // allow it

        let result = run(&a, &cfg, &pool, &b).unwrap();
        assert!(result.agents_md.is_some());
        assert!(b.project.join("AGENTS.md").exists());
        let content = std::fs::read_to_string(b.project.join("AGENTS.md")).unwrap();
        assert!(content.contains("lific:begin"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn run_skip_agents_writes_no_agents_md() {
        let dir = tmp();
        let b = base(&dir);
        std::fs::create_dir_all(&b.project).unwrap();
        let pool = db::open_memory().unwrap();
        let cfg = Config::default();
        let a = args(&["opencode"], Scope::Project); // skip_agents = true
        let result = run(&a, &cfg, &pool, &b).unwrap();
        assert!(result.agents_md.is_none());
        assert!(!b.project.join("AGENTS.md").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}

//! The subcommands `prism` runs without opening the app, with a window or without
//! (`prism-server`): their table is `args`, shared by both binaries; what each does is here.
//! `command` puts `prism` on the PATH (Settings › General › Command line).

pub mod args;
pub mod command;

use std::ffi::OsString;

pub use args::{Bin, Info};
use args::{Cmd, McpAction, Pack, Parsed, Peer, Service};

/// What the binary does after its arguments are read.
#[derive(Debug, PartialEq, Eq)]
pub enum Run {
    /// Done (a subcommand, its help, an error): exit with this code.
    Exit(i32),
    /// No subcommand: the app opens its window.
    Window,
    /// `prism-server serve`.
    Serve,
}

/// Reads `args` (the binary's name first) for `info.bin`: runs the subcommand, if any, after the
/// profile is set (`--profile`, checked).
pub fn run(info: &Info, args: Vec<OsString>) -> Run {
    // An agent's hook (`<exe> hook <folder> …`), on every event of every agent: straight to it,
    // without building the whole command line; nothing of the app started.
    if args.get(1).is_some_and(|a| a == "hook")
        && let Some(dir) = args.get(2).filter(|d| !d.to_string_lossy().starts_with('-'))
    {
        return Run::Exit(if crate::agents::hook::main(std::path::Path::new(dir)) == std::process::ExitCode::SUCCESS { 0 } else { 1 });
    }
    let cli = match args::parse(info, args) {
        Parsed::Exit(code) => return Run::Exit(code),
        Parsed::Run(cli) => cli,
    };
    if let Some(id) = &cli.profile {
        if crate::profile::exists(id) {
            crate::profile::set_from_args(id);
        } else {
            let known: Vec<String> = crate::profile::names().into_iter().map(|(id, _)| id).collect();
            let said = format!("{}: unknown profile {id:?} (profiles: {})", info.bin.name(), known.join(", "));
            // What Prism itself starts with a profile (the window, the MCP server written into
            // the agent CLIs' configs, a hook, the keep-running job) goes on with the default one,
            // as before; what is typed stops.
            if !matches!(cli.command, None | Some(Cmd::Mcp(_) | Cmd::Hook { .. } | Cmd::Watch | Cmd::Sessions)) {
                eprintln!("{said}");
                return Run::Exit(2);
            }
            eprintln!("{said}: the default one");
        }
    }
    match &cli.command {
        None if info.bin == Bin::App => Run::Window,
        None => {
            let _ = args::command(info).print_help();
            Run::Exit(2)
        }
        // The same Prism without a window: the server's, or a profile's for the app's window.
        Some(Cmd::Serve | Cmd::Engine) => Run::Serve,
        Some(cmd) => Run::Exit(subcommand(info, cmd)),
    }
}

/// A subcommand run: its exit code (1: it failed; the message already said).
fn subcommand(info: &Info, cmd: &Cmd) -> i32 {
    let bin = info.bin.name();
    if let Cmd::Hook { dir, .. } = cmd {
        return if crate::agents::hook::main(dir) == std::process::ExitCode::SUCCESS { 0 } else { 1 };
    }
    let done: anyhow::Result<Option<String>> = match cmd {
        Cmd::Serve | Cmd::Engine | Cmd::Hook { .. } => unreachable!("handled above"),
        Cmd::Pair => block_on(pair()).map(|()| None),
        Cmd::Peer(action) => peer(action).map(|()| None),
        Cmd::Restart => crate::profile::restart().map(Some),
        Cmd::Watch => crate::profile::watch(),
        Cmd::Sessions => crate::agents::tmux::daemon::main().map(|()| None),
        Cmd::Pack(action) => pack(action).map(Some),
        Cmd::Mcp(mcp) => match &mcp.action {
            None => crate::mcp::serve_stdio(false).map(|()| None),
            Some(McpAction::Aside) => crate::mcp::serve_stdio(true).map(|()| None),
            Some(McpAction::Install { cli }) => crate::mcp::install::install(cli.name()).map(Some),
            Some(McpAction::Uninstall { cli }) => crate::mcp::install::uninstall(cli.name()).map(Some),
            Some(McpAction::Status) => Ok(Some(crate::mcp::status_text())),
        },
        Cmd::Token { service } => token(*service).map(Some),
        Cmd::ClickupToken => token(Service::Clickup).map(Some),
        Cmd::JiraToken => token(Service::Jira).map(Some),
        Cmd::GoogleSecret => token(Service::Google).map(Some),
        Cmd::Completions { shell } => {
            clap_complete::generate(*shell, &mut args::command(info), bin, &mut std::io::stdout());
            Ok(None)
        }
    };
    match done {
        Ok(msg) => {
            if let Some(msg) = msg.filter(|m| !m.is_empty()) {
                println!("{msg}");
            }
            0
        }
        Err(e) => {
            eprintln!("{bin}: {e:#}");
            1
        }
    }
}

/// `prism pack import | update | remove`: config.toml's packs, as in Settings › General › Packs.
fn pack(action: &Pack) -> anyhow::Result<String> {
    let path = crate::config_path();
    block_on(async {
        match action {
            Pack::Import { source } => crate::packs::import(&path, source).await.map(|p| format!("pack {} imported", p.name)),
            Pack::Update { name, reference } => crate::packs::update(&path, name, reference.as_deref()).await.map(|p| format!("pack {} updated{}", p.name, p.commit.map(|c| format!(" at {}", &c[..c.len().min(8)])).unwrap_or_default())),
            Pack::Remove { name } => crate::packs::remove(&path, name).map(|()| format!("pack {name} removed")),
        }
    })
}

/// `prism token <service>`: the token to the system keychain (on macOS, `security
/// add-generic-password`), typed hidden on a terminal, read from standard input otherwise.
fn token(service: Service) -> anyhow::Result<String> {
    use std::io::IsTerminal;
    let (key, label) = match service {
        Service::Clickup => (crate::collectors::clickup::KEYCHAIN_SERVICE, "ClickUp token"),
        Service::Jira => (crate::collectors::jira::KEYCHAIN_SERVICE, "Jira API token"),
        Service::Google => (crate::collectors::google::SECRET_SERVICE, "Google client secret"),
    };
    let token = if std::io::stdin().is_terminal() {
        read_hidden(&format!("{label} (not shown): "))?
    } else {
        let mut t = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut t)?;
        t
    };
    match token.trim() {
        "" => anyhow::bail!("no {label} given"),
        t => crate::platform::store_secret(&crate::profile::service(key), t).map(|()| format!("{label} stored in the keychain (profile {})", crate::profile::active())),
    }
}

#[cfg(unix)]
fn read_hidden(prompt: &str) -> std::io::Result<String> {
    prism_sys::term::read_hidden(prompt)
}

#[cfg(not(unix))]
fn read_hidden(prompt: &str) -> std::io::Result<String> {
    use std::io::Write;
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line)
}

/// `prism peer add | remove | list`, on this profile's data folder.
fn peer(action: &Peer) -> anyhow::Result<()> {
    let dir = || crate::profile::data_dir(&crate::profile::data_base());
    match action {
        Peer::Pair => block_on(peer_pair()),
        Peer::Add { name } => {
            let token = prism_remote::peers::add(&dir(), name)?;
            println!("{token}");
            eprintln!("The token of {name}, shown once: paste it in its window there (profile menu › Other computers), or set it as PRISM_MACHINE_TOKEN_<NAME> where no keychain is unlocked. It opens this Prism as this account would: keep it as an SSH key.");
            Ok(())
        }
        Peer::Remove { name } => {
            anyhow::ensure!(prism_remote::peers::remove(&dir(), name)?, "no peer {name}");
            println!("{name} removed: its windows close within ten seconds");
            Ok(())
        }
        Peer::List => {
            for p in prism_remote::peers::list(&dir())? {
                println!("{}\t{}", p.name, p.created.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M"));
            }
            Ok(())
        }
    }
}

/// `prism peer pair`: a computer's pairing opened in the running Prism, its code shown here; the
/// computer that gave it accepted here by name.
async fn peer_pair() -> anyhow::Result<()> {
    use std::io::Write;
    let dir = crate::profile::data_dir(&crate::profile::data_base());
    let local = prism_remote::pair::Local::find(&dir)?;
    let pairing = local.peer_start().await?;
    let code = format!("{} {}", &pairing.code[..3], &pairing.code[3..]);
    println!("On the other computer: profile menu › Other computers › this machine, and the code {code}");
    println!("Valid until {}. Waiting for it… (Ctrl+C cancels)", pairing.expires_at.with_timezone(&chrono::Local).format("%H:%M:%S"));
    let name = loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
            _ = tokio::signal::ctrl_c() => {
                let _ = local.peer_cancel().await;
                anyhow::bail!("pairing cancelled");
            }
        }
        match local.peer_state().await? {
            None => anyhow::bail!("the pairing expired, was cancelled, or too many wrong codes were tried"),
            Some(p) => {
                if let Some(name) = p.name {
                    break name;
                }
            }
        }
    };
    print!("{name} asks to open this Prism, as this account would. Accept? [y/N] ");
    std::io::stdout().flush()?;
    // Read off the runtime, Ctrl+C still heard meanwhile: it cancels, as while waiting.
    let line = tokio::task::spawn_blocking(|| {
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer).map(|_| answer)
    });
    let answer = tokio::select! {
        a = line => a??,
        _ = tokio::signal::ctrl_c() => {
            let _ = local.peer_cancel().await;
            println!();
            anyhow::bail!("pairing cancelled");
        }
    };
    let accept = matches!(answer.trim(), "y" | "Y" | "yes" | "o" | "oui");
    local.peer_answer(accept).await?;
    anyhow::ensure!(accept, "{name} refused");
    // Its name here: another computer of that name keeps its own, this one's made unique.
    let paired = prism_remote::peers::list(&dir)?.into_iter().max_by_key(|p| p.created).map_or(name, |p| p.name);
    println!("{paired} paired: `prism peer remove \"{paired}\"` revokes it");
    Ok(())
}

/// A future run to its end, outside of Prism's runtime (none runs yet).
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("a runtime");
    let out = rt.block_on(future);
    // Not waiting for what still blocks (a question's line read, Ctrl+C having answered it).
    rt.shutdown_background();
    out
}

/// `prism pair`: the pairing opened in the running Prism, its QR code shown here; once the phone
/// has made its passkey, the code it shows is confirmed here, as on the desk.
async fn pair() -> anyhow::Result<()> {
    use std::io::Write;
    let local = prism_remote::pair::Local::find(&crate::profile::data_dir(&crate::profile::data_base()))?;
    let pairing = local.start().await?;
    println!("{}", prism_remote::pair::qr_text(&pairing.url)?);
    println!("Scan with the phone, or open: {}", pairing.url);
    println!("Valid until {}. Waiting for the phone… (Ctrl+C cancels)", pairing.expires_at.with_timezone(&chrono::Local).format("%H:%M:%S"));
    let ready = loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
            _ = tokio::signal::ctrl_c() => {
                let _ = local.cancel().await;
                anyhow::bail!("pairing cancelled");
            }
        }
        match local.state().await? {
            None => anyhow::bail!("the pairing expired, or was cancelled in Prism"),
            Some(p) if p.ready => break p,
            Some(_) => {}
        }
    };
    let (device, code) = (ready.device.unwrap_or_default(), ready.code.unwrap_or_default());
    print!("{device} shows the code {code}: the same? [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    if !matches!(answer.trim(), "y" | "Y" | "yes" | "o" | "oui") {
        let _ = local.cancel().await;
        anyhow::bail!("not the same code: the pairing is cancelled");
    }
    local.confirm(&code).await?;
    println!("{device} paired: it unlocks Prism with its passkey");
    Ok(())
}


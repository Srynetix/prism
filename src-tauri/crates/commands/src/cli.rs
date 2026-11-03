//! The subcommands `prism` runs without opening the app, with a window or without
//! (`prism-server`): `hook`, `pack`, `restart`, `watch`, `pair`, `peer`, and the tokens to the keyring.

/// Runs the subcommand of `args` (the profile already taken out), if any: its exit code.
pub fn run(args: &[std::ffi::OsString]) -> Option<i32> {
    // Called by an agent's hooks (`prism hook <folder>`): log and return
    // immediately, without starting anything of the app.
    if args.get(1).is_some_and(|a| a == "hook") {
        let code = args.get(2).map_or(std::process::ExitCode::FAILURE, |dir| crate::agents::hook::main(std::path::Path::new(dir)));
        return Some(if code == std::process::ExitCode::SUCCESS { 0 } else { 1 });
    }
    // `prism pack import <file | glab:group/project:path@ref | gh:owner/repo:path@ref | prism:starter>`, `prism pack update <name> [ref]`
    // (a repository's pack from a branch, a tag or a commit), `prism pack remove <name>`: the packs of `config.toml`, as in Settings › General › Packs.
    if args.get(1).is_some_and(|a| a == "pack") {
        let arg = |i: usize| args.get(i).and_then(|a| a.to_str()).unwrap_or_default().to_string();
        let path = crate::config_path();
        let done = block_on(async {
            match arg(2).as_str() {
                "import" => crate::packs::import(&path, &arg(3)).await.map(|p| format!("pack {} imported", p.name)),
                "update" => {
                    let reference = args.get(4).and_then(|a| a.to_str());
                    crate::packs::update(&path, &arg(3), reference).await.map(|p| format!("pack {} updated{}", p.name, p.commit.map(|c| format!(" at {}", &c[..c.len().min(8)])).unwrap_or_default()))
                }
                "remove" => crate::packs::remove(&path, &arg(3)).map(|()| format!("pack {} removed", arg(3))),
                _ => Err(anyhow::anyhow!("usage: prism pack import <file | glab:group/project:path@ref | gh:owner/repo:path@ref | prism:starter> | update <name> [branch | tag | commit] | remove <name>")),
            }
        });
        match &done {
            Ok(msg) => println!("{msg}"),
            Err(e) => eprintln!("prism: {e:#}"),
        }
        return Some(if done.is_ok() { 0 } else { 1 });
    }
    // `prism restart`: the Prism of the profile (`--profile`, otherwise the active one) restarts,
    // rereading config.toml and its packs; not open, it starts.
    if args.get(1).is_some_and(|a| a == "restart") {
        let done = crate::profile::restart();
        match &done {
            Ok(msg) => println!("{msg}"),
            Err(e) => eprintln!("prism: {e:#}"),
        }
        return Some(if done.is_ok() { 0 } else { 1 });
    }
    // `prism watch`: run every minute by the LaunchAgent of `[remote] keep_running`, starts the
    // profile's Prism at login and after a crash.
    if args.get(1).is_some_and(|a| a == "watch") {
        let done = crate::profile::watch();
        match &done {
            Ok(Some(msg)) => println!("{msg}"),
            Ok(None) => {}
            Err(e) => eprintln!("prism: {e:#}"),
        }
        return Some(if done.is_ok() { 0 } else { 1 });
    }
    // `prism pair`: a phone paired from this terminal, with the running Prism of the profile (a
    // server's, without a window to show the QR code).
    if args.get(1).is_some_and(|a| a == "pair") {
        let done = block_on(pair());
        if let Err(e) = &done {
            eprintln!("prism: {e:#}");
        }
        return Some(if done.is_ok() { 0 } else { 1 });
    }
    // `prism peer pair` (a code typed on the other computer, its name accepted here), `prism peer
    // add <name>` (its token printed, once), `prism peer remove <name>`, `prism peer list`: the other
    // Prisms that open this one in their window (`[[machines]]` there).
    if args.get(1).is_some_and(|a| a == "peer") {
        let done = if args.get(2).is_some_and(|a| a == "pair") { block_on(peer_pair()) } else { peer(args) };
        if let Err(e) = &done {
            eprintln!("prism: {e:#}");
        }
        return Some(if done.is_ok() { 0 } else { 1 });
    }
    // `prism clickup-token`, `prism jira-token`, `prism google-secret`: the token read on standard
    // input goes to the system keyring (Linux, Windows; on macOS, `security add-generic-password`).
    let keychain = match args.get(1).and_then(|a| a.to_str()) {
        Some("clickup-token") => Some(crate::collectors::clickup::KEYCHAIN_SERVICE),
        Some("jira-token") => Some(crate::collectors::jira::KEYCHAIN_SERVICE),
        Some("google-secret") => Some(crate::collectors::google::SECRET_SERVICE),
        _ => None,
    };
    if let Some(service) = keychain {
        let mut token = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut token);
        let stored = match token.trim() {
            "" => Err(anyhow::anyhow!("no token on standard input")),
            t => crate::platform::store_secret(&crate::profile::service(service), t),
        };
        if let Err(e) = &stored {
            eprintln!("prism: {e:#}");
        }
        return Some(if stored.is_ok() { 0 } else { 1 });
    }
    None
}

/// `prism peer …`, on this profile's data folder.
fn peer(args: &[std::ffi::OsString]) -> anyhow::Result<()> {
    let arg = |i: usize| args.get(i).and_then(|a| a.to_str()).unwrap_or_default().to_string();
    let dir = crate::profile::data_dir(&crate::profile::data_base());
    match (arg(2).as_str(), arg(3)) {
        ("add", name) if !name.is_empty() => {
            let token = prism_remote::peers::add(&dir, &name)?;
            println!("{token}");
            eprintln!("The token of {name}, shown once: paste it in its window there (profile menu › Other computers), or set it as PRISM_MACHINE_TOKEN_<NAME> where no keychain is unlocked. It opens this Prism as this account would: keep it as an SSH key.");
            Ok(())
        }
        ("remove", name) if !name.is_empty() => {
            anyhow::ensure!(prism_remote::peers::remove(&dir, &name)?, "no peer {name}");
            println!("{name} removed: its windows close within ten seconds");
            Ok(())
        }
        ("list", _) => {
            for p in prism_remote::peers::list(&dir)? {
                println!("{}\t{}", p.name, p.created.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M"));
            }
            Ok(())
        }
        _ => anyhow::bail!("usage: prism peer pair | add <name> | remove <name> | list"),
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

#[cfg(test)]
mod tests {
    fn args(list: &[&str]) -> Vec<std::ffi::OsString> {
        std::iter::once("prism").chain(list.iter().copied()).map(Into::into).collect()
    }

    /// No subcommand, or another binary's (`serve`): the binary goes on; a wrong use ends it.
    #[test]
    fn subcommands_and_the_rest() {
        assert_eq!(super::run(&args(&[])), None);
        assert_eq!(super::run(&args(&["serve"])), None);
        assert_eq!(super::run(&args(&["pack", "nonsense"])), Some(1));
        assert_eq!(super::run(&args(&["hook"])), Some(1), "no folder");
    }
}

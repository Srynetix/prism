//! The Dev servers block in the demo profile: the saved-cart agent runs the shop's stack, the VAT
//! one lost its API to a port already taken; a Postgres and an old prototype run outside Prism.

use chrono::{DateTime, Duration, Utc};

use super::{DevServer, DevServersState, Listener};

pub fn state(now: DateTime<Utc>) -> DevServersState {
    let server = |key: &str, agent: &str, shell: Option<u32>, command: &str, program: &str, ports: &[u16], minutes: i64| DevServer {
        key: key.into(),
        agent: agent.into(),
        shell,
        typed: shell.is_some(),
        command: command.into(),
        program: program.into(),
        ports: ports.to_vec(),
        state: "running".into(),
        tail: None,
        failed: false,
        docker: false,
        since: Some(now - Duration::minutes(minutes)),
    };
    let outside = |port: u16, pid: u32, program: &str, command: &str, docker: bool| Listener { port, pid, program: program.into(), command: command.into(), docker };
    DevServersState {
        servers: vec![
            server("shell:demo-a2:2", "demo-a2", Some(2), "just api-dev", "uv", &[8000], 130),
            server("shell:demo-a2:3", "demo-a2", Some(3), "pnpm --dir web dev", "node", &[5173], 128),
            DevServer { docker: true, ..server("docker:shop-saved-cart-db-1", "demo-a2", None, "postgres:16", "postgres:16", &[5433], 131) },
            DevServer {
                state: "stopped".into(),
                ports: vec![],
                tail: Some("INFO:     Started reloader process [48211]\nERROR:    [Errno 48] error while attempting to bind on address ('127.0.0.1', 8000): address already in use".into()),
                failed: true,
                ..server("shell:demo-a1:1", "demo-a1", Some(1), "just api-dev", "zsh", &[], 6)
            },
            server("shell:demo-a1:2", "demo-a1", Some(2), "pnpm --dir web dev", "node", &[5174], 6),
        ],
        outside: vec![outside(5432, 812, "postgres", "/opt/homebrew/opt/postgresql@16/bin/postgres -D /opt/homebrew/var/postgresql@16", false), outside(3000, 40211, "node", "node ~/old-proto/server.js", false)],
        scan_outside: true,
        read_at: Some(now),
    }
}

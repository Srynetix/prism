//! The Docker block in the demo profile.

use chrono::{DateTime, Duration, Utc};

use super::{DockerContainer, DockerDisk, DockerImage, DockerState, DockerVolume};

/// A local Docker: the shop's Compose project (its worker unhealthy), a stopped standalone container.
pub fn state(now: DateTime<Utc>) -> DockerState {
    let ago = |minutes: i64| now - Duration::minutes(minutes);
    let container = |id: &str, name: &str, image: &str, project: Option<&str>, state: &str, status: &str, health: Option<&str>, exit: Option<i32>, ports: &[&str], cpu: Option<f64>, mem: Option<u64>| DockerContainer {
        id: id.into(),
        name: name.into(),
        image: image.into(),
        state: state.into(),
        status: status.into(),
        health: health.map(String::from),
        exit_code: exit,
        project: project.map(String::from),
        ports: ports.iter().map(|p| p.to_string()).collect(),
        created_at: Some(ago(60 * 24 * 3)),
        size_bytes: Some(2_400_000),
        cpu_percent: cpu,
        mem_bytes: mem,
        mem_percent: cpu.map(|_| 2.5),
    };
    let image = |id: &str, name: &str, size: u64, unique: u64, containers: u32| DockerImage { id: id.into(), name: name.into(), size_bytes: size, unique_bytes: Some(unique), containers, created_at: Some(ago(60 * 24 * 12)) };
    let volume = |name: &str, size: u64, links: u32| DockerVolume { name: name.into(), size_bytes: Some(size), links };
    let disk = |kind: &str, total: u32, active: u32, size: u64, reclaimable: u64| DockerDisk { kind: kind.into(), total, active, size_bytes: size, reclaimable_bytes: reclaimable };
    DockerState {
        context: "orbstack".into(),
        server: Some("29.4.0".into()),
        containers: vec![
            container("c1", "shop-api-1", "shop/api:dev", Some("shop"), "running", "Up 3 days (healthy)", Some("healthy"), None, &["8080→8080/tcp"], Some(1.8), Some(310_000_000)),
            container("c2", "shop-db-1", "postgres:16", Some("shop"), "running", "Up 3 days (healthy)", Some("healthy"), None, &["5432→5432/tcp"], Some(0.4), Some(95_000_000)),
            container("c3", "shop-worker-1", "shop/worker:dev", Some("shop"), "running", "Up 2 minutes (unhealthy)", Some("unhealthy"), None, &[], Some(12.5), Some(420_000_000)),
            container("c4", "mailpit", "axllent/mailpit:latest", None, "exited", "Exited (0) 2 days ago", None, Some(0), &[], None, None),
        ],
        images: vec![image("i1", "postgres:16", 438_000_000, 438_000_000, 1), image("i2", "shop/api:dev", 1_210_000_000, 640_000_000, 1), image("i3", "shop/worker:dev", 1_190_000_000, 620_000_000, 1), image("i4", "<none>", 1_150_000_000, 1_150_000_000, 0)],
        volumes: vec![volume("shop_pgdata", 2_300_000_000, 1), volume("old-project_pgdata", 9_800_000_000, 0), volume("3f9c1e0a7b", 41_000_000, 0)],
        disk: vec![disk("images", 4, 3, 3_988_000_000, 1_150_000_000), disk("containers", 4, 3, 9_600_000, 2_400_000), disk("volumes", 3, 1, 12_141_000_000, 9_841_000_000), disk("build_cache", 18, 0, 2_600_000_000, 2_600_000_000)],
        disk_at: Some(ago(2)),
        reclaimable_gb: 10.0,
    }
}

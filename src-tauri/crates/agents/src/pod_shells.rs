//! A pod's shells (its logs pane): `kubectl exec` into a container, kept by tmux like a worktree's
//! shells, with the rights of the kubeconfig. Which pods may be reached is the caller's to check.

use super::*;

/// What runs in the container: bash when the image has it, sh otherwise.
const IN_POD: &str = "command -v bash >/dev/null 2>&1 && exec bash || exec sh";

/// `kubectl` run by sh, its arguments given apart (never parsed by a shell). A failure (no rights,
/// no shell in the image, pod gone) leaves its message on screen until Enter, rather than a
/// session gone at once with it.
const WRAPPER: &str = r#"kubectl "$@"; code=$?; [ $code -eq 0 ] && exit 0; printf '\n\033[2m[kubectl exited with %s · Enter closes]\033[0m' "$code"; read -r _"#;

/// A pod, as the logs pane names it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PodRef {
    pub context: String,
    pub namespace: String,
    pub pod: String,
    pub container: Option<String>,
}

impl PodRef {
    fn owner(&self) -> String {
        pod_owner(&self.context, &self.namespace, &self.pod, self.container.as_deref())
    }

    /// `kubectl exec -it` into the pod. Names of Kubernetes objects only (no option slipped in);
    /// the context as `--context=`, whatever its name.
    fn argv(&self) -> Result<Vec<String>> {
        let name = |what: &str, s: &str| {
            let ok = !s.is_empty() && s.len() <= 253 && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.') && s.as_bytes()[0].is_ascii_alphanumeric();
            if ok { Ok(()) } else { Err(anyhow::anyhow!("not a {what} name: {s}")) }
        };
        name("namespace", &self.namespace)?;
        name("pod", &self.pod)?;
        ensure!(!self.context.is_empty(), "no kube context");
        let mut argv: Vec<String> = ["sh", "-c", WRAPPER, "kubectl"].map(String::from).into();
        argv.extend([format!("--context={}", self.context), format!("--namespace={}", self.namespace), "exec".into(), "-it".into(), self.pod.clone()]);
        if let Some(c) = &self.container {
            name("container", c)?;
            argv.push(format!("--container={c}"));
        }
        argv.extend(["--".into(), "sh".into(), "-c".into(), IN_POD.into()]);
        Ok(argv)
    }

    /// What a tab shows: the container, or the pod.
    fn label(&self) -> String {
        self.container.clone().unwrap_or_else(|| self.pod.clone())
    }
}

impl Agents {
    /// The pod's shells still open. Each runs `kubectl` (its foreground program): none is "busy",
    /// the tab names the container.
    pub async fn pod_shells(&self, pod: &PodRef) -> Result<Vec<Shell>> {
        let id = pod.owner();
        let shells = self.inner.tmux.shells(&id).await?;
        Ok(shells.into_iter().map(|(n, _)| Shell { key: shell_key(&id, n), n, busy: false, command: pod.label() }).collect())
    }

    /// Opens the pod's shell `n` (again once it ended), or a new one (`None`): its terminal key.
    pub async fn pod_shell(&self, pod: &PodRef, n: Option<u32>) -> Result<String> {
        let argv = pod.argv()?;
        let id = pod.owner();
        let tmux = &self.inner.tmux;
        let n = match n {
            Some(n) => n,
            None => tmux.shells(&id).await?.iter().map(|(n, _)| n + 1).max().unwrap_or(1),
        };
        let key = shell_key(&id, n);
        if tmux.pane(&key).await?.is_none() {
            let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
            tmux.new_session(&key, &home, &[], &argv).await?;
            tmux.end_with_program(&key).await?;
            // Its fingerprint does not say which pod: `open_shells` reads it here.
            tmux.set_note(&key, "prism-pod", &serde_json::to_string(pod)?).await?;
        }
        Ok(key)
    }

    /// Closes the pod's shell `n`: its `kubectl exec` ends, and what it ran in the container.
    pub async fn close_pod_shell(&self, pod: &PodRef, n: u32) -> Result<()> {
        let key = shell_key(&pod.owner(), n);
        self.detach(&key);
        self.inner.tmux.kill(&key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pod(container: Option<&str>) -> PodRef {
        PodRef { context: "gke_proj_europe-west1_prod".into(), namespace: "shop".into(), pod: "api-7d9f-x2".into(), container: container.map(String::from) }
    }

    /// kubectl's arguments stay arguments: positional to the wrapper, the context as one `--context=`.
    #[test]
    fn execs_into_the_pod() {
        let argv = pod(Some("app")).argv().unwrap();
        assert_eq!(&argv[..2], ["sh", "-c"]);
        assert_eq!(argv[3], "kubectl");
        assert_eq!(argv[4..10], ["--context=gke_proj_europe-west1_prod", "--namespace=shop", "exec", "-it", "api-7d9f-x2", "--container=app"]);
        assert_eq!(argv.last().unwrap(), IN_POD);
        assert!(!pod(None).argv().unwrap().iter().any(|a| a.starts_with("--container")));
    }

    #[test]
    fn refuses_what_is_not_a_name() {
        for bad in ["--all", "", "Shop", "a b", "x;rm"] {
            let mut p = pod(None);
            p.pod = bad.into();
            assert!(p.argv().is_err(), "{bad}");
        }
        let mut p = pod(Some("-c"));
        assert!(p.argv().is_err());
        p.container = None;
        p.context = String::new();
        assert!(p.argv().is_err());
    }
}

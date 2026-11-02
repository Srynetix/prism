//! What the Agents view says of an agent: its title, kind, tags, ticket and MR.

use super::*;

impl Agents {
    /// Renames the agent in Prism; Claude keeps the name given at launch (`-n`).
    pub fn rename(&self, id: &str, title: &str) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            bail!(tr("the name is empty"));
        }
        self.edit_record(id, |r| r.title = title.to_string())
    }

    /// The kind its prompt's command makes (`[[agents.kinds]] when`), if any.
    pub(crate) fn kind_of(&self, prompt: Option<&str>) -> Option<String> {
        self.inner.settings.lock().unwrap().kind_of(prompt?).map(String::from)
    }

    /// Sets its reasoning effort (`None`: its CLI's), kept for its resumes; a Claude Code agent at
    /// its prompt takes it at once (`/effort <level>`). Whether it did: otherwise it runs with it
    /// from its next resume (Codex, which changes it only in its own model picker; an agent at work).
    pub async fn set_effort(&self, id: &str, effort: Option<String>) -> Result<bool> {
        let provider = self.provider(id)?;
        ensure!(provider.takes_effort(), "{} has no effort to choose", provider.program());
        let effort = effort.map(|e| e.trim().to_lowercase()).filter(|e| !e.is_empty());
        if let Some(e) = &effort {
            ensure!(prism_core::agents::models::valid_effort(e), "{e} is not an effort level");
        }
        self.edit_record(id, |r| r.launch.get_or_insert_with(Default::default).effort = effort.clone())?;
        let running = self.inner.hub.subscribe().borrow().agents.iter().any(|a| a.id == id && a.state == state::AgentState::Running);
        match effort {
            Some(e) if provider == Provider::Claude && running => Ok(self.slash(id, tr("change its effort"), &format!("/effort {e}")).await.is_ok()),
            _ => Ok(false),
        }
    }

    /// Gives the agent a kind, or takes it away (`None`, or an empty name).
    pub fn set_category(&self, id: &str, category: Option<String>) -> Result<()> {
        let category = category.map(|c| c.trim().to_string()).filter(|c| !c.is_empty());
        self.edit_record(id, |r| r.category = category)
    }

    /// Sets the agent's tags: trimmed, without empty ones nor duplicates, in the order given.
    pub fn set_tags(&self, id: &str, tags: Vec<String>) -> Result<()> {
        let tags = clean_tags(tags);
        self.edit_record(id, |r| r.tags = tags)
    }

    /// Links the agent to a ticket by hand, over the one found in its title or branch; `None`
    /// (or an empty id) unlinks it, back to that one.
    pub fn link_ticket(&self, id: &str, ticket: Option<String>) -> Result<()> {
        let ticket = ticket.map(|t| t.trim().to_uppercase()).filter(|t| !t.is_empty());
        self.edit_record(id, |r| r.ticket = ticket)
    }

    /// Links the agent to an MR by hand, over its branch's; `None` unlinks it.
    pub fn link_mr(&self, id: &str, mr: Option<crate::state::MrLink>) -> Result<()> {
        self.edit_record(id, |r| r.mr = mr)
    }

    /// A kind or a tag renamed (Agents view › Manage): every agent carrying it follows.
    /// An empty `to` takes it off them.
    pub fn relabel(&self, kind: Label, from: &str, to: &str) -> Result<()> {
        self.update_registry(|reg| relabel(&mut reg.records, kind, from, to))
    }

    pub(crate) fn edit_record(&self, id: &str, f: impl FnOnce(&mut Record)) -> Result<()> {
        if self.inner.registry.lock().unwrap().get(id).is_none() {
            bail!(tr("unknown agent"));
        }
        self.update_registry(|reg| {
            if let Some(r) = reg.records.iter_mut().find(|r| r.id == id) {
                f(r);
            }
        })
    }
}

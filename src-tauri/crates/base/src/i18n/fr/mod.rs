//! The French tables, one per crate; the first that knows a text gives its translation.

mod agents;
mod app;
mod blocks;
mod collectors;
mod core;
mod remote;

const TABLES: &[&[(&str, &str)]] = &[app::TABLE, core::TABLE, collectors::TABLE, agents::TABLE, blocks::TABLE, remote::TABLE];

pub fn get(text: &str) -> Option<&'static str> {
    TABLES.iter().find_map(|t| t.iter().find(|(en, _)| *en == text).map(|(_, fr)| *fr))
}

#[cfg(test)]
pub fn all() -> Vec<(&'static str, &'static str)> {
    TABLES.iter().flat_map(|t| t.iter().copied()).collect()
}

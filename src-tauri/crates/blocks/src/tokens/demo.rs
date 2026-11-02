//! The Tokens block in the demo profile: two months of plausible days, quieter at weekends, mostly
//! Claude Code's, some Codex's.

use chrono::{DateTime, Datelike, Duration, Local, Utc};

use super::{Day, TokensState};

pub fn state(now: DateTime<Utc>) -> TokensState {
    let today = now.with_timezone(&Local).date_naive();
    let days = (0..62)
        .rev()
        .filter_map(|ago| {
            let date = today - Duration::days(ago);
            let weekend = date.weekday().number_from_monday() > 5;
            // A deterministic wave rather than noise: the same demo every time.
            let level = ((ago * 37 % 11) as u64 + 4) * if weekend { 1 } else { 6 };
            (!weekend || ago % 3 == 0).then(|| {
                let date = date.format("%Y-%m-%d").to_string();
                let claude = tool_day(&date, "claude", level * 4, 0.75e-6, &[("claude-opus-5-5", 3), ("claude-sonnet-5-5", 1)], &[("shop", 3), ("payments", 1), ("docs", 1)]);
                let codex = tool_day(&date, "codex", level, 0.3e-6, &[("gpt-5-codex", 1)], &[("payments", 2), ("docs", 1)]);
                let mut day = Day { date: date.clone(), ..Default::default() };
                for d in [&claude, &codex] {
                    day.input += d.input;
                    day.output += d.output;
                    day.cache_read += d.cache_read;
                    day.cache_write += d.cache_write;
                    day.replies += d.replies;
                    day.cost += d.cost;
                    day.unpriced += d.unpriced;
                    day.tools.extend(d.tools.iter().cloned());
                    for (list, from) in [(&mut day.models, &d.models), (&mut day.projects, &d.projects)] {
                        for (k, n) in from {
                            match list.iter_mut().find(|(x, _)| x == k) {
                                Some((_, m)) => *m += n,
                                None => list.push((k.clone(), *n)),
                            }
                        }
                    }
                }
                day.models.sort_by_key(|a| std::cmp::Reverse(a.1));
                day.projects.sort_by_key(|a| std::cmp::Reverse(a.1));
                day.by_tool = [("claude".to_string(), claude), ("codex".to_string(), codex)].into();
                day
            })
        })
        .collect();
    TokensState { days, oldest: None, read_at: Some(now) }
}

/// One tool's day at `level`, its tokens split between models and projects by weight, at `rate` dollars a token.
fn tool_day(date: &str, tool: &str, level: u64, rate: f64, models: &[(&str, u64)], projects: &[(&str, u64)]) -> Day {
    let (input, output) = (level * 2_250, level * 5_250);
    let (cache_read, cache_write) = (level * 475_000, level * 35_000);
    let total = input + output + cache_read + cache_write;
    let split = |parts: &[(&str, u64)]| {
        let weight: u64 = parts.iter().map(|p| p.1).sum();
        let mut v: Vec<(String, u64)> = parts.iter().map(|(k, w)| (k.to_string(), total * w / weight)).collect();
        v[0].1 += total - v.iter().map(|p| p.1).sum::<u64>();
        v
    };
    Day {
        date: date.to_string(),
        input,
        output,
        cache_read,
        cache_write,
        replies: (level * 3) as u32,
        cost: total as f64 * rate,
        unpriced: 0,
        models: split(models),
        projects: split(projects),
        tools: vec![(tool.to_string(), total)],
        by_tool: Default::default(),
    }
}

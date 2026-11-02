//! The Day trail in the demo profile: a week of Alex's traces on the shop, today's up to now.

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone, Utc};

use super::{Kind, TrailEvent, TrailState};

/// `day`: working days back from today (0: today); a trace still to come today is left out.
pub fn state(now: DateTime<Utc>) -> TrailState {
    let today = now.with_timezone(&Local).date_naive();
    // The `back`-th working day before today.
    let workday = |back: u32| {
        let mut d = today;
        let mut left = back;
        while left > 0 {
            d -= Duration::days(1);
            if d.weekday().number_from_monday() <= 5 {
                left -= 1;
            }
        }
        d
    };
    let at = |back: u32, hh: u32, mm: u32| Local.from_local_datetime(&workday(back).and_time(NaiveTime::from_hms_opt(hh, mm, 0).unwrap())).single().map(|t| t.with_timezone(&Utc));
    let blank = |id: String, at: DateTime<Utc>, kind: Kind, title: &str| TrailEvent { id, at, kind, title: title.into(), ticket: None, repo: None, mr: None, others: None, url: None, from: None, to: None, branch: None, minutes: None };
    let mut events = Vec::new();
    let mut push = |e: Option<TrailEvent>| events.extend(e.filter(|e| e.at <= now));

    let commit = |back, hh, mm, n: u32, title: &str, ticket: u32, branch: &str| {
        at(back, hh, mm).map(|t| TrailEvent { ticket: Some(format!("SHOP-{ticket}")), repo: Some("shop".into()), branch: Some(branch.into()), ..blank(format!("commit:demo-{back}-{n}"), t, Kind::Commit, title) })
    };
    let mr = |back, hh, mm, kind: Kind, iid: u64, title: &str, ticket: u32, others: bool| {
        let id = match kind {
            Kind::MrOpened => format!("mr_opened:shop:{iid}"),
            Kind::MrMerged => format!("mr_merged:shop:{iid}"),
            Kind::MrApproved => format!("mr_approved:shop:{iid}"),
            _ => format!("gitlab:demo-{back}-{hh}{mm}-{iid}"),
        };
        at(back, hh, mm).map(|t| TrailEvent {
            ticket: Some(format!("SHOP-{ticket}")),
            repo: Some("shop".into()),
            mr: Some(iid),
            others: Some(others),
            url: Some(format!("https://gitlab.example.org/example-org/shop/-/merge_requests/{iid}")),
            ..blank(id, t, kind, title)
        })
    };
    let moved = |back, hh, mm, ticket: u32, title: &str, from: &str, to: &str| {
        at(back, hh, mm).map(|t| TrailEvent { ticket: Some(format!("SHOP-{ticket}")), from: Some(from.into()), to: Some(to.into()), ..blank(format!("move:demo-{ticket}-{to}"), t, Kind::TicketMoved, title) })
    };
    let meeting = |back, hh, mm, minutes: u32, title: &str| at(back, hh, mm).map(|t| TrailEvent { minutes: Some(minutes), ..blank(format!("meeting:demo-{back}-{hh}{mm}"), t, Kind::Meeting, title) });

    // Four days ago: the order pagination review, a hotfix.
    push(meeting(4, 9, 30, 15, "Daily stand-up"));
    push(mr(4, 10, 5, Kind::MrCommented, 1280, "SHOP-430 Search the catalog by SKU", 430, true));
    push(mr(4, 10, 40, Kind::MrApproved, 1280, "SHOP-430 Search the catalog by SKU", 430, true));
    push(commit(4, 14, 10, 1, "fix: payment page blank on Safari 16", 439, "hotfix/SHOP-439-safari"));
    push(mr(4, 14, 30, Kind::MrOpened, 1286, "SHOP-439 Payment page blank on Safari 16", 439, false));
    push(mr(4, 16, 50, Kind::MrMerged, 1286, "SHOP-439 Payment page blank on Safari 16", 439, false));
    // Three days ago: the saved cart.
    push(meeting(3, 9, 30, 15, "Daily stand-up"));
    push(moved(3, 9, 50, 412, "Checkout: save the cart between sessions", "todo", "doing"));
    push(commit(3, 11, 20, 1, "feat: keep the cart in orders-api", 412, "feat/SHOP-412-saved-cart"));
    push(commit(3, 15, 5, 2, "feat: merge the guest cart at sign-in", 412, "feat/SHOP-412-saved-cart"));
    push(commit(3, 21, 40, 3, "test: the merged cart keeps three products", 412, "feat/SHOP-412-saved-cart"));
    // Two days ago: the saved cart in review, Maya's images reviewed.
    push(meeting(2, 9, 30, 15, "Daily stand-up"));
    push(mr(2, 10, 15, Kind::MrOpened, 1288, "SHOP-412 Save the cart between sessions", 412, false));
    push(moved(2, 10, 16, 412, "Checkout: save the cart between sessions", "doing", "review"));
    push(mr(2, 14, 0, Kind::MrCommented, 1294, "Draft: SHOP-425 Lazy-load product images", 425, true));
    push(meeting(2, 16, 0, 60, "Sprint 42 refinement"));
    // Yesterday: the VAT bug.
    push(meeting(1, 9, 30, 15, "Daily stand-up"));
    push(moved(1, 9, 48, 433, "Fix VAT rounding on invoices", "todo", "doing"));
    push(commit(1, 10, 34, 1, "test: VAT per line, failing test", 433, "fix/SHOP-433-vat-rounding"));
    push(commit(1, 11, 50, 2, "fix: round each line, not the total", 433, "fix/SHOP-433-vat-rounding"));
    push(mr(1, 14, 5, Kind::MrCommented, 1290, "SHOP-418 Paginate the order history", 418, true));
    push(mr(1, 14, 25, Kind::MrCommented, 1290, "SHOP-418 Paginate the order history", 418, true));
    push(commit(1, 16, 30, 3, "fix: invoices total from the rounded lines", 433, "fix/SHOP-433-vat-rounding"));
    push(mr(1, 16, 45, Kind::MrOpened, 1291, "SHOP-433 Fix VAT rounding on invoices", 433, false));
    // Today: the saved cart merged, Sam's pagination reviewed, a gap after lunch.
    push(meeting(0, 9, 30, 15, "Daily stand-up"));
    push(commit(0, 9, 55, 1, "fix: address the review of the saved cart", 412, "feat/SHOP-412-saved-cart"));
    push(meeting(0, 11, 0, 30, "1:1 with Sam"));
    push(mr(0, 13, 42, Kind::MrCommented, 1290, "SHOP-418 Paginate the order history", 418, true));
    push(mr(0, 14, 20, Kind::MrMerged, 1288, "SHOP-412 Save the cart between sessions", 412, false));
    push(moved(0, 14, 21, 412, "Checkout: save the cart between sessions", "review", "to validate"));
    push(mr(0, 14, 50, Kind::MrApproved, 1292, "SHOP-427 Export orders as CSV", 427, true));
    push(meeting(0, 15, 0, 60, "Architecture review: order events"));
    push(commit(0, 16, 20, 2, "fix: VAT rounding on credit notes too", 433, "fix/SHOP-433-vat-rounding"));
    events.sort_by_key(|e| e.at);
    TrailState { events, gap_minutes: 60, sources: ["commits", "forge", "tickets", "meetings"].map(String::from).to_vec(), read_at: Some(now) }
}

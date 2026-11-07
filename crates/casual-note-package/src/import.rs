//! Brings a package into a workspace as one transaction (docs/36). A plan is a list of ordinary
//! ops, tried on a scratch copy first: if any would fail, nothing happens to the workspace. The
//! web app shows the report, then runs the same ops through its one write path, so undo and saving
//! work. Ids are kept. If any id already exists, everything arrives as a copy with new ids and its
//! links rewritten. Import never merges, because a package has no shared history.

use crate::read::{Package, Report, read_package};
use crate::{PackageError, refuse};
use casual_note_merge::Workspace;
use casual_note_model::{BlockKind, Note, Page, VIEW_PAGE};
use casual_note_ops::{Op, apply};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// An id that had to change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Renamed {
    pub from: String,
    pub to: String,
}

/// What an import will do, for the person to see before it commits.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    pub pages: usize,
    pub databases: usize,
    pub blocks: usize,
    pub strokes: usize,
    /// True when ids clashed, so everything arrives as a copy.
    pub copied: bool,
    pub renamed: Vec<Renamed>,
    /// Links to pages in neither the package nor the workspace. They are kept, and show as removed.
    pub dangling: Vec<String>,
    /// Page titles, for the person to recognise. First few only.
    pub titles: Vec<String>,
    /// The page to open afterwards.
    pub first_page: Option<String>,
    #[serde(flatten)]
    pub read: Report,
}

/// The ops to run, and what they will do.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ImportPlan {
    pub ops: Vec<Op>,
    pub report: ImportReport,
}

/// A new id for a copy. Derived rather than random, so planning twice gives the same plan.
fn copy_id(old: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let mut round = 0u64;
    loop {
        let mut hash = Sha256::new();
        hash.update(round.to_le_bytes());
        hash.update(old.as_bytes());
        let id: String = hash
            .finalize()
            .iter()
            .take(12)
            .map(|b| format!("{b:02x}"))
            .collect();
        if !taken(&id) {
            return id;
        }
        round += 1;
    }
}

/// Parents before children. A missing parent or a cycle lands the page at the top level, reported.
fn creation_order(pages: &[Page], report: &mut Report) -> Vec<(usize, Option<String>)> {
    let ids: BTreeMap<&str, usize> = pages
        .iter()
        .enumerate()
        .map(|(i, p)| (p.meta.id.as_str(), i))
        .collect();
    let mut children: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut roots = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        match page.meta.parent.as_deref() {
            None => roots.push((i, None)),
            Some(parent) => match ids.get(parent) {
                Some(p) if *p != i => children.entry(*p).or_default().push(i),
                _ => {
                    report.moved_to_top.push(page.meta.id.clone());
                    roots.push((i, None));
                }
            },
        }
    }
    let mut order = Vec::with_capacity(pages.len());
    let mut placed = BTreeSet::new();
    let mut stack = roots;
    let mut next = 0usize;
    loop {
        while let Some((i, parent)) = stack.pop() {
            if !placed.insert(i) {
                continue;
            }
            order.push((i, parent));
            let parent_id = pages.get(i).map(|p| p.meta.id.clone());
            for child in children.get(&i).into_iter().flatten().rev() {
                stack.push((*child, parent_id.clone()));
            }
        }
        // Anything still unplaced is on a cycle: break it at the lowest id, and carry on.
        while next < pages.len() && placed.contains(&next) {
            next += 1;
        }
        let Some(page) = pages.get(next) else { break };
        report.moved_to_top.push(page.meta.id.clone());
        stack.push((next, None));
    }
    order
}

/// Plans an import and proves it on a scratch copy. `scratch_peer` is a fresh peer for that copy,
/// which is discarded. The workspace is not touched.
pub fn plan_import(
    ws: &Workspace,
    mut package: Package,
    scratch_peer: u64,
) -> Result<ImportPlan, PackageError> {
    let existing_pages: BTreeSet<String> = ws.pages().into_iter().map(|m| m.id).collect();
    let existing_databases: BTreeSet<String> = ws.database_ids().into_iter().collect();
    let copied = package
        .pages
        .iter()
        .any(|p| existing_pages.contains(&p.meta.id))
        || package
            .databases
            .iter()
            .any(|d| existing_databases.contains(&d.id));
    let mut report = ImportReport {
        copied,
        ..ImportReport::default()
    };
    let mut pages_map: BTreeMap<String, String> = BTreeMap::new();
    let mut databases_map: BTreeMap<String, String> = BTreeMap::new();
    if copied {
        let assign = |ids: Vec<String>,
                      map: &mut BTreeMap<String, String>,
                      existing: &BTreeSet<String>,
                      renamed: &mut Vec<Renamed>| {
            for old in ids {
                let id = {
                    let used = map.values().cloned().collect::<BTreeSet<String>>();
                    copy_id(&old, &|c| existing.contains(c) || used.contains(c))
                };
                renamed.push(Renamed {
                    from: old.clone(),
                    to: id.clone(),
                });
                map.insert(old, id);
            }
        };
        let mut sorted: Vec<String> = package.pages.iter().map(|p| p.meta.id.clone()).collect();
        sorted.sort();
        assign(sorted, &mut pages_map, &existing_pages, &mut report.renamed);
        let mut sorted: Vec<String> = package.databases.iter().map(|d| d.id.clone()).collect();
        sorted.sort();
        assign(
            sorted,
            &mut databases_map,
            &existing_databases,
            &mut report.renamed,
        );
    }
    let in_package: BTreeSet<String> = package.pages.iter().map(|p| p.meta.id.clone()).collect();
    let mut dangling = BTreeSet::new();
    let mut read_report = std::mem::take(&mut package.report);
    package.pages.sort_by(|a, b| a.meta.id.cmp(&b.meta.id));
    let order = creation_order(&package.pages, &mut read_report);

    let mut ops = Vec::new();
    for mut database in std::mem::take(&mut package.databases) {
        if let Some(new) = databases_map.get(&database.id) {
            database.id.clone_from(new);
        }
        report.databases += 1;
        ops.push(Op::CreateDatabase { database });
    }
    let rewrite_page = |id: &str| pages_map.get(id).cloned().unwrap_or_else(|| id.to_string());
    let rewrite_db = |id: &str| {
        databases_map
            .get(id)
            .cloned()
            .unwrap_or_else(|| id.to_string())
    };
    for (index, parent) in order {
        let Some(page) = package.pages.get(index) else {
            continue;
        };
        let id = rewrite_page(&page.meta.id);
        ops.push(Op::CreatePage {
            id: id.clone(),
            parent: parent.as_deref().map(&rewrite_page),
            title: page.meta.title.clone(),
            database: page.meta.database.as_deref().map(&rewrite_db),
        });
        if report.first_page.is_none() {
            report.first_page = Some(id.clone());
        }
        if report.titles.len() < 12 {
            report.titles.push(page.meta.title.clone());
        }
        report.pages += 1;
        if page.meta.favorite {
            ops.push(Op::SetFavorite {
                id: id.clone(),
                value: true,
            });
        }
        if page.view != VIEW_PAGE {
            ops.push(Op::SetView {
                page: id.clone(),
                view: page.view.clone(),
            });
        }
        if page.canvas.note != Note::default() {
            ops.push(Op::SetNote {
                page: id.clone(),
                note: page.canvas.note.clone(),
            });
        }
        for item in &page.canvas.items {
            ops.push(Op::PutItem {
                page: id.clone(),
                item: item.clone(),
            });
        }
        if !page.canvas.ink.is_empty() {
            report.strokes += page.canvas.ink.len();
            ops.push(Op::ChangeInk {
                page: id.clone(),
                add: page.canvas.ink.clone(),
                remove: Vec::new(),
            });
        }
        let mut after: Option<String> = None;
        for block in &page.blocks {
            let mut block = block.clone();
            for run in &mut block.runs {
                if let Some(link) = &mut run.link {
                    if !in_package.contains(link.as_str())
                        && !existing_pages.contains(link.as_str())
                    {
                        dangling.insert(link.clone());
                    }
                    *link = rewrite_page(link);
                }
            }
            match block.kind {
                BlockKind::Embed => {
                    if let Some(target) = &mut block.target {
                        if !in_package.contains(target.as_str())
                            && !existing_pages.contains(target.as_str())
                        {
                            dangling.insert(target.clone());
                        }
                        *target = rewrite_page(target);
                    }
                }
                BlockKind::Database => {
                    if let Some(target) = &mut block.target {
                        *target = rewrite_db(target);
                    }
                }
                _ => {}
            }
            let block_id = block.id.clone();
            report.blocks += 1;
            ops.push(Op::InsertBlock {
                page: id.clone(),
                after: after.clone(),
                block,
            });
            after = Some(block_id);
        }
    }
    report.dangling = dangling.into_iter().collect();
    report.read = read_report;

    let bad = |e: casual_note_model::Error| refuse("CN-PKG-0010", &e.to_string());
    let mut scratch = Workspace::new(scratch_peer).map_err(bad)?;
    scratch.import(&ws.export().map_err(bad)?).map_err(bad)?;
    for op in &ops {
        apply(&mut scratch, op).map_err(bad)?;
    }
    Ok(ImportPlan { ops, report })
}

/// Reads a package and imports it. All or nothing: the ops run only after the scratch copy took
/// every one of them.
pub fn import_package(
    ws: &mut Workspace,
    data: &[u8],
    scratch_peer: u64,
) -> Result<ImportReport, PackageError> {
    let plan = plan_import(ws, read_package(data)?, scratch_peer)?;
    for op in &plan.ops {
        apply(ws, op).map_err(|e| refuse("CN-PKG-0010", &e.to_string()))?;
    }
    Ok(plan.report)
}

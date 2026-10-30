//! Page-level three-way merge. Concurrent edits to one page require an explicit choice.
use crate::{Note, Workspace};

#[derive(Debug, PartialEq)]
pub struct Conflicts(pub Vec<String>);

/// A missing baseline is deliberately conservative for drafts created before sync existed.
pub fn merge(
    base: Option<&Workspace>,
    local: &Workspace,
    remote: &Workspace,
    mut recover_id: Option<&mut dyn FnMut() -> String>,
) -> Result<Workspace, Conflicts> {
    let base = base.filter(|b| b.revision == local.revision);
    let base = if local.revision == remote.revision {
        Some(remote)
    } else {
        base
    };
    let find = |w: &Workspace, id: &str| w.notes.iter().find(|n| n.id == id).cloned();
    let mut notes = Vec::new();
    let mut conflicts = Vec::new();
    for id in remote.notes.iter().chain(&local.notes).map(|n| &n.id) {
        if notes.iter().any(|n: &Note| &n.id == id) || conflicts.contains(id) {
            continue;
        }
        let l = find(local, id);
        let r = find(remote, id);
        let b = base.and_then(|w| find(w, id));
        let choice = if l == r {
            Some(l.clone())
        } else if base.is_some() && l == b {
            Some(r.clone())
        } else if base.is_some() && r == b {
            Some(l.clone())
        } else if base.is_none() && (l.is_none() || r.is_none()) {
            // Without a baseline, absence cannot prove deletion. Retain the surviving page.
            Some(l.clone().or(r.clone()))
        } else {
            None
        };
        if let Some(note) = choice {
            notes.extend(note);
        } else {
            conflicts.push(id.clone());
            if let Some(make_id) = recover_id.as_mut() {
                if let Some(mut n) = l {
                    if r.is_some() {
                        n.id = make_id();
                        if local
                            .notes
                            .iter()
                            .chain(&remote.notes)
                            .chain(&notes)
                            .any(|old| old.id == n.id)
                        {
                            return Err(Conflicts(vec![id.clone()]));
                        }
                        // Reserve bytes for the suffix without splitting a UTF-8 character.
                        while n.title.len() > 490 {
                            n.title.pop();
                        }
                        n.title.push_str(" (device copy)");
                    }
                    notes.push(n);
                }
                notes.extend(r);
            }
        }
    }
    if !conflicts.is_empty() && recover_id.is_none() {
        return Err(Conflicts(conflicts));
    }
    Ok(Workspace {
        schema: remote.schema,
        revision: remote.revision,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pair() -> Workspace {
        let mut w = Workspace::default();
        let mut n = w.notes[0].clone();
        n.id = "second".into();
        n.title = "Second".into();
        w.notes.push(n);
        w
    }
    #[test]
    fn different_pages_merge_but_same_page_requires_choice() {
        let base = pair();
        let mut local = base.clone();
        let mut remote = base.clone();
        local.notes[0].body = "offline writing".into();
        remote.notes[1].body = "other device".into();
        remote.revision = 1;
        let merged = merge(Some(&base), &local, &remote, None).unwrap();
        assert_eq!(merged.notes[0], local.notes[0]);
        assert_eq!(merged.notes[1], remote.notes[1]);
        remote.notes[0].body = "concurrent writing".into();
        assert_eq!(
            merge(Some(&base), &local, &remote, None).unwrap_err().0,
            vec![base.notes[0].id.clone()]
        );
        let both = merge(
            Some(&base),
            &local,
            &remote,
            Some(&mut || "recovered".into()),
        )
        .unwrap();
        both.validate().unwrap();
        assert_eq!(both.notes.len(), 3);
        assert!(
            both.notes
                .iter()
                .any(|n| n.id == "recovered" && n.body == "offline writing")
        );
        assert!(
            both.notes
                .iter()
                .any(|n| n.id == base.notes[0].id && n.body == "concurrent writing")
        );
    }
    #[test]
    fn recovery_id_collision_never_drops_an_existing_page() {
        let base = pair();
        let mut local = base.clone();
        let mut remote = base.clone();
        local.notes[0].body = "local".into();
        remote.notes[0].body = "remote".into();
        remote.revision = 1;
        assert!(merge(Some(&base), &local, &remote, Some(&mut || "second".into())).is_err());
        assert_eq!(local.notes.len(), 2);
        assert_eq!(remote.notes.len(), 2);
    }
    #[test]
    fn deletions_additions_legacy_drafts_and_stale_baselines() {
        let base = pair();
        let mut local = base.clone();
        let mut remote = base.clone();
        local.notes.remove(1);
        remote.revision = 3;
        assert_eq!(
            merge(Some(&base), &local, &remote, None).unwrap().notes,
            local.notes
        );
        remote.notes[1].body = "edited after deletion".into();
        assert!(merge(Some(&base), &local, &remote, None).is_err());
        let both = merge(Some(&base), &local, &remote, Some(&mut || "unused".into())).unwrap();
        assert_eq!(both.notes, remote.notes);
        assert!(merge(None, &local, &remote, None).is_ok()); // No inferred deletion in old drafts.
        local.notes[0].body = "old unsynced text".into();
        assert!(merge(None, &local, &remote, None).is_err());
        let mut stale = base.clone();
        stale.revision = 99;
        assert!(merge(Some(&stale), &local, &remote, None).is_err());
        local.revision = remote.revision;
        assert_eq!(
            merge(None, &local, &remote, None).unwrap().notes,
            local.notes
        );
    }
}

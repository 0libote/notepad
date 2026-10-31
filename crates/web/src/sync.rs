use crate::{Editor, persistence, uid};
use leptos::prelude::*;
use notepad_core::{AuthState, Workspace, sync::merge};
use wasm_bindgen_futures::spawn_local;

#[derive(Clone, Copy)]
pub struct Sync {
    editor: Editor,
    enabled: RwSignal<bool>,
    auth: RwSignal<Option<AuthState>>,
    busy: RwSignal<bool>,
    epoch: RwSignal<u64>,
    pub running: RwSignal<bool>,
    pub status: RwSignal<String>,
    pub conflicts: RwSignal<usize>,
}
impl Sync {
    pub fn new(
        editor: Editor,
        enabled: RwSignal<bool>,
        auth: RwSignal<Option<AuthState>>,
        busy: RwSignal<bool>,
        epoch: RwSignal<u64>,
    ) -> Self {
        Self {
            editor,
            enabled,
            auth,
            busy,
            epoch,
            running: RwSignal::new(false),
            status: RwSignal::new(String::new()),
            conflicts: RwSignal::new(0),
        }
    }
    pub fn start(self) {
        spawn_local(async move {
            let mut seen = 0;
            let mut attempted = None;
            let mut poll = 0;
            loop {
                gloo_timers::future::TimeoutFuture::new(1000).await;
                let generation = self.editor.local_generation.get_untracked();
                poll += 1;
                if generation != seen {
                    seen = generation;
                    continue;
                }
                if attempted != Some(generation) || poll >= 15 {
                    attempted = Some(generation);
                    poll = 0;
                    if self.conflicts.get_untracked() == 0 {
                        self.request(false);
                    }
                }
            }
        });
    }
    pub fn request(self, recover: bool) {
        if !self.editor.ready.get_untracked()
            || !self.enabled.get_untracked()
            || self.auth.get_untracked().is_some_and(|a| !a.authenticated)
            || self.busy.get_untracked()
            || self.running.get_untracked()
        {
            return;
        }
        self.editor.status.set(String::new());
        self.running.set(true);
        self.status.set("Syncing…".into());
        let epoch = self.epoch.get_untracked();
        spawn_local(async move {
            let result = self.run(epoch, recover).await;
            if self.epoch.get_untracked() == epoch
                && let Err(error) = result
            {
                self.status.set(error);
                if let Ok(state) = persistence::auth_state().await
                    && self.epoch.get_untracked() == epoch
                {
                    self.auth.set(Some(state));
                }
            }
            self.running.set(false);
        });
    }
    fn valid(self, epoch: u64) -> bool {
        self.epoch.get_untracked() == epoch
            && self.editor.ready.get_untracked()
            && !self.busy.get_untracked()
    }
    fn apply(self, w: Workspace, base: Workspace) {
        let e = self.editor;
        let changed = e.workspace.with_untracked(|old| old.notes != w.notes);
        let active = e
            .workspace
            .with_untracked(|old| old.notes[e.active.get_untracked()].id.clone());
        let needs_save =
            e.workspace.get_untracked() != w || e.sync_base.get_untracked().as_ref() != Some(&base);
        if changed {
            e.active
                .set(w.notes.iter().position(|n| n.id == active).unwrap_or(0));
            e.history.set(vec![]);
            e.future.set(vec![]);
            e.workspace.set(w);
            e.workspace_epoch.update(|v| *v += 1);
        } else {
            e.workspace.update(|old| old.revision = w.revision);
            e.history.update(|h| {
                for old in h {
                    old.revision = w.revision;
                }
            });
        }
        e.sync_base.set(Some(base));
        if needs_save {
            e.persist();
        }
    }
    async fn run(self, epoch: u64, recover: bool) -> Result<(), String> {
        if self.auth.get_untracked().is_none() {
            let state = persistence::auth_state().await?;
            if !self.valid(epoch) {
                return Ok(());
            }
            let authenticated = state.authenticated;
            self.auth.set(Some(state));
            if !authenticated {
                return Err("Sign in to sync. Your local draft is kept.".into());
            }
        }
        let remote = persistence::load_server().await?;
        if !self.valid(epoch) {
            return Ok(());
        }
        let local = self.editor.workspace.get_untracked();
        let base = self.editor.sync_base.get_untracked();
        let mut id = uid;
        let merged = match merge(
            base.as_ref(),
            &local,
            &remote,
            if recover { Some(&mut id) } else { None },
        ) {
            Ok(w) => w,
            Err(conflicts) => {
                self.conflicts.set(conflicts.0.len());
                self.status.set(format!(
                    "{} page(s) changed on both devices. Keep both versions to continue.",
                    conflicts.0.len()
                ));
                return Ok(());
            }
        };
        merged.validate().map_err(|e| {
            format!("Sync paused: {e}. Export both notebooks before merging manually.")
        })?;
        let dirty = merged.notes != remote.notes;
        self.apply(merged.clone(), remote);
        self.conflicts.set(0);
        if dirty {
            // Never upload work that failed to reach this device's durable store.
            let generation = self.editor.local_generation.get_untracked();
            while self.editor.local_running.get_untracked()
                && self.editor.local_saved.get_untracked() < generation
            {
                gloo_timers::future::TimeoutFuture::new(20).await;
                if !self.valid(epoch) {
                    return Ok(());
                }
            }
            if self.editor.local_saved.get_untracked() < generation {
                return Err("Sync paused: local save failed. Export a backup and retry.".into());
            }
            if !self.valid(epoch) {
                return Ok(());
            }
            let saved = persistence::save_server(&merged).await?;
            if !self.valid(epoch) {
                return Ok(());
            }
            // Typing can continue during the request. Only advance its revision and baseline.
            let mut current = self.editor.workspace.get_untracked();
            current.revision = saved.revision;
            self.apply(current, saved);
        }
        let pending = self.editor.sync_base.with_untracked(|base| {
            base.as_ref().is_some_and(|base| {
                self.editor
                    .workspace
                    .with_untracked(|w| w.notes != base.notes)
            })
        });
        self.status.set(
            if pending {
                "Local edits waiting to sync"
            } else {
                "Synced with server"
            }
            .into(),
        );
        Ok(())
    }
}

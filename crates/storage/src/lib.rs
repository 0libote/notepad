mod client;
pub use client::DesktopClient;
use notepad_core::Workspace;
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

/// Drafts are scoped to a server identity so switching hosts cannot mix revisions.
pub struct DraftStore(Connection);
impl DraftStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let db = Connection::open(path).map_err(|e| e.to_string())?;
        db.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS drafts (scope TEXT PRIMARY KEY, json TEXT NOT NULL);").map_err(|e|e.to_string())?;
        Ok(Self(db))
    }
    pub fn load(&self, scope: &str) -> Result<Option<Workspace>, String> {
        let json: Option<String> = self
            .0
            .query_row("SELECT json FROM drafts WHERE scope=?1", [scope], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())?;
        json.map(|json| {
            let w: Workspace = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            w.upgrade().map_err(str::to_string)
        })
        .transpose()
    }
    pub fn save(&mut self, scope: &str, w: &Workspace) -> Result<(), String> {
        self.save_pair(scope, w, None)
    }
    pub fn load_base(&self, scope: &str) -> Result<Option<Workspace>, String> {
        self.load(&format!("{scope}#sync-base"))
    }
    pub fn save_pair(
        &mut self,
        scope: &str,
        w: &Workspace,
        base: Option<&Workspace>,
    ) -> Result<(), String> {
        w.validate().map_err(str::to_string)?;
        if let Some(b) = base {
            b.validate().map_err(str::to_string)?;
            if b.revision != w.revision {
                return Err("Sync baseline revision differs from the draft".into());
            }
        }
        let json = serde_json::to_string(w).map_err(|e| e.to_string())?;
        let tx = self.0.transaction().map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO drafts (scope,json) VALUES (?1,?2) ON CONFLICT(scope) DO UPDATE SET json=excluded.json",rusqlite::params![scope,json]).map_err(|e|e.to_string())?;
        let key = format!("{scope}#sync-base");
        if let Some(b) = base {
            let json = serde_json::to_string(b).map_err(|e| e.to_string())?;
            tx.execute("INSERT INTO drafts (scope,json) VALUES (?1,?2) ON CONFLICT(scope) DO UPDATE SET json=excluded.json",rusqlite::params![key,json]).map_err(|e|e.to_string())?;
        } else {
            tx.execute("DELETE FROM drafts WHERE scope=?1", [key])
                .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stored_version_one_drafts_and_baselines_upgrade_on_read() {
        let store = DraftStore::open(":memory:").unwrap();
        let mut old = Workspace::default();
        old.schema = 1;
        old.revision = 7;
        old.notes[0].body = "Old offline work".into();
        let json = serde_json::to_string(&old).unwrap();
        for scope in ["one", "one#sync-base"] {
            store
                .0
                .execute(
                    "INSERT INTO drafts (scope,json) VALUES (?1,?2)",
                    rusqlite::params![scope, json],
                )
                .unwrap();
        }
        let w = store.load("one").unwrap().unwrap();
        assert_eq!(w.schema, notepad_core::SCHEMA);
        assert_eq!(w.revision, 7);
        assert_eq!(w.notes[0].body, "Old offline work");
        assert_eq!(store.load_base("one").unwrap(), Some(w));
    }
    #[test]
    fn draft_and_sync_baseline_commit_together() {
        let mut store = DraftStore::open(":memory:").unwrap();
        let base = Workspace::default();
        let mut draft = base.clone();
        draft.notes[0].body = "offline text".into();
        store.save_pair("one", &draft, Some(&base)).unwrap();
        assert_eq!(store.load("one").unwrap(), Some(draft.clone()));
        assert_eq!(store.load_base("one").unwrap(), Some(base.clone()));
        assert_eq!(store.load_base("two").unwrap(), None);
        let mut invalid = base.clone();
        invalid.revision = 99;
        assert!(store.save_pair("one", &base, Some(&invalid)).is_err());
        assert_eq!(store.load("one").unwrap(), Some(draft.clone()));
        assert_eq!(store.load_base("one").unwrap(), Some(base));
        store.save_pair("one", &draft, None).unwrap();
        assert_eq!(store.load_base("one").unwrap(), None);
    }
    #[test]
    fn durable_scoped_drafts_and_failed_validation() {
        let path = std::env::temp_dir().join(format!("notepad-drafts-{}.db", std::process::id()));
        {
            let mut store = DraftStore::open(&path).unwrap();
            let mut w = Workspace::default();
            w.notes[0].body = "Unsynced local work".into();
            w.revision = 9;
            store.save("https://one.example", &w).unwrap();
            assert!(store.load("https://two.example").unwrap().is_none());
            w.schema = 999;
            assert!(store.save("https://one.example", &w).is_err());
        }
        let store = DraftStore::open(&path).unwrap();
        let w = store.load("https://one.example").unwrap().unwrap();
        assert_eq!(w.notes[0].body, "Unsynced local work");
        assert_eq!(w.revision, 9);
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
}

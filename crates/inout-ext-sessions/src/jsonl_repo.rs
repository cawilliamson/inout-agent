//! newline-delimited json session repository.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::fs::{self, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::entry::{EntryBase, SessionEntry};
use crate::repo::{
    ForkOptions, ListOptions, MessageContext, NavigateTreeResult, SessionContext, SessionInfo,
    SessionMetadata, SessionRepo,
};

/// file-backed jsonl session repo.
#[derive(Debug)]
pub struct JsonlSessionRepo {
    /// directory holding the journal and metadata.
    dir: PathBuf,
    /// in-memory index of all entries by id.
    entries: Mutex<HashMap<String, SessionEntry>>,
    /// current leaf id.
    leaf_id: Mutex<String>,
}

impl JsonlSessionRepo {
    /// create a new repo at `dir`, initialising it if empty.
    pub async fn new(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir).await?;

        let journal = journal_path(&dir);
        let (entries, leaf_id) = if journal.exists() {
            load_journal(&journal).await?
        } else {
            let root = SessionEntry::Leaf(crate::entry::LeafEntry {
                base: EntryBase { id: uuid(), parent_id: None, timestamp: now() },
                leaf_id: String::new(),
            });
            let leaf_id = entry_id(&root).to_string();
            let mut entries = HashMap::new();
            entries.insert(leaf_id.clone(), root.clone());
            Self::append_to_journal(&journal, &root).await?;
            (entries, leaf_id)
        };

        Ok(Self { dir, entries: Mutex::new(entries), leaf_id: Mutex::new(leaf_id) })
    }

    /// create a new repo at `dir` synchronously, for use in non-async
    /// contexts like extension `register` methods. uses `std::fs`.
    pub fn new_blocking(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;

        let journal = journal_path(&dir);
        let (entries, leaf_id) = if journal.exists() {
            load_journal_blocking(&journal)?
        } else {
            let root = SessionEntry::Leaf(crate::entry::LeafEntry {
                base: EntryBase { id: uuid(), parent_id: None, timestamp: now() },
                leaf_id: String::new(),
            });
            let leaf_id = entry_id(&root).to_string();
            let mut entries = HashMap::new();
            entries.insert(leaf_id.clone(), root.clone());
            append_to_journal_blocking(&journal, &root)?;
            (entries, leaf_id)
        };

        Ok(Self { dir, entries: Mutex::new(entries), leaf_id: Mutex::new(leaf_id) })
    }

    /// path to the journal file.
    pub fn journal_path(&self) -> PathBuf {
        journal_path(&self.dir)
    }

    async fn append_to_journal(journal: &Path, entry: &SessionEntry) -> Result<()> {
        let mut file = OpenOptions::new().create(true).append(true).open(journal).await?;

        let line = serde_json::to_string(entry).context("serialise entry")?;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;
        Ok(())
    }

    fn index_entry(&self, entry: &SessionEntry) {
        let mut entries = self.entries.lock().expect("entries lock poisoned");
        let id = entry_id(entry).to_string();
        entries.insert(id, entry.clone());
    }

    fn walk_to_root(&self, leaf_id: &str) -> Vec<SessionEntry> {
        let entries = self.entries.lock().expect("entries lock poisoned");
        let mut path = Vec::new();
        let mut current = leaf_id.to_string();

        while let Some(entry) = entries.get(&current) {
            path.push(entry.clone());
            match parent_id(entry) {
                Some(parent) => current = parent.to_string(),
                None => break,
            }
        }

        path
    }
}

#[async_trait]
impl SessionRepo for JsonlSessionRepo {
    async fn append_entry(&self, entry: SessionEntry) -> Result<()> {
        let id = entry_id(&entry).to_string();
        let parent_id = parent_id(&entry).map(String::from);
        let is_message = matches!(&entry, SessionEntry::Message(_));

        self.index_entry(&entry);
        Self::append_to_journal(&self.journal_path(), &entry).await?;

        if is_message {
            // messages become the new tip; persist via a leaf marker.
            let tip_id = id.clone();
            let marker = SessionEntry::Leaf(crate::entry::LeafEntry {
                base: EntryBase { id: uuid(), parent_id: Some(tip_id.clone()), timestamp: now() },
                leaf_id: tip_id,
            });
            let marker_id = entry_id(&marker).to_string();
            self.index_entry(&marker);
            Self::append_to_journal(&self.journal_path(), &marker).await?;

            let mut leaf_id = self.leaf_id.lock().expect("leaf_id lock poisoned");
            *leaf_id = marker_id;
        } else if matches!(&entry, SessionEntry::Leaf(_)) {
            let mut leaf_id = self.leaf_id.lock().expect("leaf_id lock poisoned");
            *leaf_id = id;
        }

        // avoid unused warning for parent_id loaded from journal reconstruction.
        let _ = parent_id;

        Ok(())
    }

    async fn fork(&self, target_id: &str, opts: ForkOptions) -> Result<String> {
        {
            let entries = self.entries.lock().expect("entries lock poisoned");
            if !entries.contains_key(target_id) {
                anyhow::bail!("fork target not found: {target_id}");
            }
        }

        let branch = SessionEntry::BranchSummary(crate::entry::BranchSummaryEntry {
            base: EntryBase {
                id: uuid(),
                parent_id: Some(target_id.to_string()),
                timestamp: now(),
            },
            branch: opts.name,
            summary: String::new(),
        });
        self.append_entry(branch).await?;

        let new_leaf = SessionEntry::Leaf(crate::entry::LeafEntry {
            base: EntryBase {
                id: uuid(),
                parent_id: Some(target_id.to_string()),
                timestamp: now(),
            },
            leaf_id: String::new(),
        });
        let new_id = entry_id(&new_leaf).to_string();
        self.append_entry(new_leaf).await?;

        Ok(new_id)
    }

    async fn list(&self, opts: ListOptions) -> Result<Vec<SessionInfo>> {
        let entries = self.entries.lock().expect("entries lock poisoned");
        let leaf_id = self.leaf_id.lock().expect("leaf_id lock poisoned");

        let mut infos: Vec<SessionInfo> = entries
            .values()
            .filter_map(|e| match e {
                SessionEntry::Leaf(leaf) => {
                    let id = leaf.base.id.clone();
                    if let Some(prefix) = &opts.prefix {
                        if !id.starts_with(prefix) {
                            return None;
                        }
                    }
                    let title =
                        if id == *leaf_id { format!("{} (active)", id) } else { id.clone() };
                    Some(SessionInfo { id, title, leaf_id: leaf.base.id.clone() })
                }
                _ => None,
            })
            .collect();

        drop(entries);
        drop(leaf_id);

        infos.sort_by(|a, b| a.id.cmp(&b.id));
        if let Some(limit) = opts.limit {
            infos.truncate(limit);
        }
        Ok(infos)
    }

    async fn navigate_tree(&self, from: &str, to: &str) -> Result<NavigateTreeResult> {
        {
            let entries = self.entries.lock().expect("entries lock poisoned");
            if !entries.contains_key(from) || !entries.contains_key(to) {
                anyhow::bail!("navigation endpoint missing");
            }
        }

        let path = self.walk_to_root(to);
        let leaf_id = to.to_string();
        self.set_leaf_id(to).await?;

        Ok(NavigateTreeResult { leaf_id, path })
    }

    async fn build_context(&self, leaf_id: &str) -> Result<SessionContext> {
        {
            let entries = self.entries.lock().expect("entries lock poisoned");
            if !entries.contains_key(leaf_id) {
                anyhow::bail!("leaf not found: {leaf_id}");
            }
        }

        let entries = self.entries.lock().expect("entries lock poisoned");
        let mut ctx = SessionContext::default();
        let mut current = leaf_id.to_string();

        // walk leaf-to-root, collecting entries that contribute to context.
        while let Some(entry) = entries.get(&current) {
            match entry {
                SessionEntry::Message(m) => ctx
                    .messages
                    .push(MessageContext { role: m.role.clone(), content: m.content.clone() }),
                SessionEntry::ModelChange(m) => ctx.model = Some(m.model.clone()),
                SessionEntry::ActiveToolsChange(t) => ctx.tools = t.tools.clone(),
                SessionEntry::Compaction(c) => {
                    ctx.messages.push(MessageContext {
                        role: "system".to_string(),
                        content: format!(
                            "[compacted {} messages] {}",
                            c.compacted_count, c.summary
                        ),
                    });
                }
                _ => {}
            }

            match parent_id(entry) {
                Some(parent) => current = parent.to_string(),
                None => break,
            }
        }

        ctx.messages.reverse();
        Ok(ctx)
    }

    async fn set_leaf_id(&self, target_id: &str) -> Result<()> {
        {
            let entries = self.entries.lock().expect("entries lock poisoned");
            if !entries.contains_key(target_id) {
                anyhow::bail!("target not found: {target_id}");
            }
        }

        {
            let mut leaf_id = self.leaf_id.lock().expect("leaf_id lock poisoned");
            *leaf_id = target_id.to_string();
        }

        let marker = SessionEntry::Leaf(crate::entry::LeafEntry {
            base: EntryBase {
                id: uuid(),
                parent_id: Some(target_id.to_string()),
                timestamp: now(),
            },
            leaf_id: target_id.to_string(),
        });

        self.index_entry(&marker);
        Self::append_to_journal(&self.journal_path(), &marker).await?;

        Ok(())
    }

    async fn get_metadata(&self) -> Result<SessionMetadata> {
        let leaf_id = self.leaf_id.lock().expect("leaf_id lock poisoned");
        Ok(SessionMetadata { leaf_id: leaf_id.clone(), branch: String::from("main") })
    }
}

fn journal_path(dir: &Path) -> PathBuf {
    dir.join("session.jsonl")
}

fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn entry_id(entry: &SessionEntry) -> &str {
    match entry {
        SessionEntry::Message(e) => &e.base.id,
        SessionEntry::ModelChange(e) => &e.base.id,
        SessionEntry::ThinkingLevelChange(e) => &e.base.id,
        SessionEntry::ActiveToolsChange(e) => &e.base.id,
        SessionEntry::Compaction(e) => &e.base.id,
        SessionEntry::BranchSummary(e) => &e.base.id,
        SessionEntry::Custom(e) => &e.base.id,
        SessionEntry::CustomMessage(e) => &e.base.id,
        SessionEntry::Label(e) => &e.base.id,
        SessionEntry::SessionInfo(e) => &e.base.id,
        SessionEntry::Leaf(e) => &e.base.id,
    }
}

fn parent_id(entry: &SessionEntry) -> Option<&str> {
    match entry {
        SessionEntry::Message(e) => e.base.parent_id.as_deref(),
        SessionEntry::ModelChange(e) => e.base.parent_id.as_deref(),
        SessionEntry::ThinkingLevelChange(e) => e.base.parent_id.as_deref(),
        SessionEntry::ActiveToolsChange(e) => e.base.parent_id.as_deref(),
        SessionEntry::Compaction(e) => e.base.parent_id.as_deref(),
        SessionEntry::BranchSummary(e) => e.base.parent_id.as_deref(),
        SessionEntry::Custom(e) => e.base.parent_id.as_deref(),
        SessionEntry::CustomMessage(e) => e.base.parent_id.as_deref(),
        SessionEntry::Label(e) => e.base.parent_id.as_deref(),
        SessionEntry::SessionInfo(e) => e.base.parent_id.as_deref(),
        SessionEntry::Leaf(e) => e.base.parent_id.as_deref(),
    }
}

async fn load_journal(journal: &Path) -> Result<(HashMap<String, SessionEntry>, String)> {
    let file = fs::File::open(journal).await?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let mut entries: HashMap<String, SessionEntry> = HashMap::new();
    let mut leaf_id = String::new();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let entry: SessionEntry =
            serde_json::from_str(&line).context("deserialise journal line")?;
        let id = entry_id(&entry).to_string();

        if let SessionEntry::Leaf(leaf) = &entry {
            leaf_id = leaf.leaf_id.clone();
            if leaf_id.is_empty() {
                leaf_id = id.clone();
            }
        }

        entries.insert(id.clone(), entry);

        if let Some(SessionEntry::Message(m)) = entries.get(&id) {
            if let Some(parent) = &m.base.parent_id {
                if !entries.contains_key(parent) {
                    let marker = SessionEntry::Leaf(crate::entry::LeafEntry {
                        base: EntryBase { id: parent.clone(), parent_id: None, timestamp: 0 },
                        leaf_id: parent.clone(),
                    });
                    entries.insert(parent.clone(), marker);
                }
            }
        }
    }

    if leaf_id.is_empty() {
        leaf_id =
            entries.keys().next().cloned().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    }

    Ok((entries, leaf_id))
}

/// synchronous version of `load_journal` for non-async contexts.
fn load_journal_blocking(journal: &Path) -> Result<(HashMap<String, SessionEntry>, String)> {
    let file = std::fs::File::open(journal)?;
    let reader = std::io::BufReader::new(file);

    let mut entries: HashMap<String, SessionEntry> = HashMap::new();
    let mut leaf_id = String::new();

    for line in std::io::BufRead::lines(reader) {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: SessionEntry =
            serde_json::from_str(&line).context("deserialise journal line")?;
        let id = entry_id(&entry).to_string();

        if let SessionEntry::Leaf(leaf) = &entry {
            leaf_id = leaf.leaf_id.clone();
            if leaf_id.is_empty() {
                leaf_id = id.clone();
            }
        }

        entries.insert(id.clone(), entry);

        if let Some(SessionEntry::Message(m)) = entries.get(&id) {
            if let Some(parent) = &m.base.parent_id {
                if !entries.contains_key(parent) {
                    let marker = SessionEntry::Leaf(crate::entry::LeafEntry {
                        base: EntryBase { id: parent.clone(), parent_id: None, timestamp: 0 },
                        leaf_id: parent.clone(),
                    });
                    entries.insert(parent.clone(), marker);
                }
            }
        }
    }

    if leaf_id.is_empty() {
        leaf_id =
            entries.keys().next().cloned().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    }

    Ok((entries, leaf_id))
}

/// synchronous version of `append_to_journal` for non-async contexts.
fn append_to_journal_blocking(journal: &Path, entry: &SessionEntry) -> Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(journal)?;

    let line = serde_json::to_string(entry).context("serialise entry")?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::entry::MessageEntry;

    use super::*;

    fn message(parent_id: Option<String>, role: &str, content: &str) -> SessionEntry {
        SessionEntry::Message(MessageEntry {
            base: EntryBase { id: uuid::Uuid::new_v4().to_string(), parent_id, timestamp: 0 },
            role: role.to_string(),
            content: content.to_string(),
        })
    }

    #[tokio::test]
    async fn append_and_reopen() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = JsonlSessionRepo::new(tmp.path()).await.unwrap();

        let leaf = repo.get_metadata().await.unwrap().leaf_id;
        let m1 = message(Some(leaf.clone()), "user", "hello");
        repo.append_entry(m1.clone()).await.unwrap();

        let repo2 = JsonlSessionRepo::new(tmp.path()).await.unwrap();
        let ctx = repo2.build_context(&repo2.get_metadata().await.unwrap().leaf_id).await.unwrap();
        assert_eq!(ctx.messages.len(), 1);
        assert_eq!(ctx.messages[0].content, "hello");
    }

    #[tokio::test]
    async fn fork_creates_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = JsonlSessionRepo::new(tmp.path()).await.unwrap();

        let leaf = repo.get_metadata().await.unwrap().leaf_id;
        let m1 = message(Some(leaf.clone()), "user", "hello");
        repo.append_entry(m1).await.unwrap();

        let new_leaf = repo.fork(&leaf, ForkOptions { name: "feature".to_string() }).await.unwrap();
        assert_ne!(new_leaf, leaf);

        let list = repo.list(ListOptions::default()).await.unwrap();
        assert_eq!(list.len(), 3);
    }
}

use anyhow::Result;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::params;

pub const ROOT_NODE_UUID: &str = "00000000-0000-0000-0000-000000000000";
pub const DEFAULT_DOMAIN: &str = "core";
pub const SYSTEM_DOMAIN: &str = "system";

const MIGRATION_0001_CORE: &str = r#"
CREATE TABLE IF NOT EXISTS schema_migrations (
  version TEXT PRIMARY KEY,
  applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS nodes (
  uuid TEXT PRIMARY KEY,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS memories (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  node_uuid TEXT NOT NULL,
  content TEXT NOT NULL,
  deprecated BOOLEAN NOT NULL DEFAULT FALSE,
  migrated_to INTEGER NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (node_uuid) REFERENCES nodes(uuid)
);

CREATE INDEX IF NOT EXISTS idx_memories_node_uuid ON memories(node_uuid);
CREATE INDEX IF NOT EXISTS idx_memories_node_uuid_deprecated ON memories(node_uuid, deprecated);

CREATE TABLE IF NOT EXISTS edges (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  parent_uuid TEXT NOT NULL,
  child_uuid TEXT NOT NULL,
  name TEXT NOT NULL,
  priority INTEGER NOT NULL DEFAULT 0,
  disclosure TEXT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (parent_uuid) REFERENCES nodes(uuid),
  FOREIGN KEY (child_uuid) REFERENCES nodes(uuid),
  CONSTRAINT uq_edges_parent_child UNIQUE (parent_uuid, child_uuid)
);

CREATE INDEX IF NOT EXISTS idx_edges_parent_uuid ON edges(parent_uuid);
CREATE INDEX IF NOT EXISTS idx_edges_child_uuid ON edges(child_uuid);

CREATE TABLE IF NOT EXISTS paths (
  domain TEXT NOT NULL,
  path TEXT NOT NULL,
  edge_id INTEGER NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (domain, path),
  FOREIGN KEY (edge_id) REFERENCES edges(id)
);

CREATE INDEX IF NOT EXISTS idx_paths_edge_id ON paths(edge_id);

CREATE TABLE IF NOT EXISTS glossary_keywords (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  keyword TEXT NOT NULL,
  node_uuid TEXT NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (node_uuid) REFERENCES nodes(uuid) ON DELETE CASCADE,
  CONSTRAINT uq_glossary_keyword_node UNIQUE (keyword, node_uuid)
);

CREATE INDEX IF NOT EXISTS idx_glossary_keywords_node_uuid ON glossary_keywords(node_uuid);
CREATE INDEX IF NOT EXISTS idx_glossary_keywords_keyword ON glossary_keywords(keyword);
"#;

const MIGRATION_0002_SEARCH: &str = r#"
CREATE TABLE IF NOT EXISTS search_documents (
  domain TEXT NOT NULL,
  path TEXT NOT NULL,
  node_uuid TEXT NOT NULL,
  memory_id INTEGER NOT NULL,
  uri TEXT NOT NULL,
  content TEXT NOT NULL,
  disclosure TEXT NULL,
  search_terms TEXT NOT NULL DEFAULT '',
  priority INTEGER NOT NULL DEFAULT 0,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (domain, path),
  FOREIGN KEY (node_uuid) REFERENCES nodes(uuid) ON DELETE CASCADE,
  FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_search_documents_node_uuid ON search_documents(node_uuid);
CREATE INDEX IF NOT EXISTS idx_search_documents_memory_id ON search_documents(memory_id);
"#;

const MIGRATION_0003_SEARCH_FTS: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS search_documents_fts USING fts5 (
  domain,
  path,
  uri,
  content,
  disclosure,
  search_terms,
  tokenize = 'unicode61'
);
"#;

const MIGRATION_0004_EDGES_ALLOW_ALIAS_NAME: &str = r#"
CREATE TABLE IF NOT EXISTS edges_new (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  parent_uuid TEXT NOT NULL,
  child_uuid TEXT NOT NULL,
  name TEXT NOT NULL,
  priority INTEGER NOT NULL DEFAULT 0,
  disclosure TEXT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (parent_uuid) REFERENCES nodes(uuid),
  FOREIGN KEY (child_uuid) REFERENCES nodes(uuid),
  CONSTRAINT uq_edges_parent_child_name UNIQUE (parent_uuid, child_uuid, name)
);

INSERT INTO edges_new (id, parent_uuid, child_uuid, name, priority, disclosure, created_at)
SELECT id, parent_uuid, child_uuid, name, priority, disclosure, created_at
FROM edges;

DROP TABLE edges;
ALTER TABLE edges_new RENAME TO edges;

CREATE INDEX IF NOT EXISTS idx_edges_parent_uuid ON edges(parent_uuid);
CREATE INDEX IF NOT EXISTS idx_edges_child_uuid ON edges(child_uuid);
"#;

const MIGRATION_0005_AUDIT_LOG: &str = r#"
CREATE TABLE IF NOT EXISTS audit_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  action TEXT NOT NULL,
  uri TEXT,
  node_uuid TEXT,
  details TEXT,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_audit_log_action_created_at ON audit_log(action, created_at);
CREATE INDEX IF NOT EXISTS idx_audit_log_node_uuid_created_at ON audit_log(node_uuid, created_at);
"#;

const MIGRATION_0006_NAMESPACE_COMPAT: &str = r#"
CREATE TABLE IF NOT EXISTS paths_new (
  namespace TEXT NOT NULL DEFAULT '',
  domain TEXT NOT NULL,
  path TEXT NOT NULL,
  edge_id INTEGER NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (namespace, domain, path),
  FOREIGN KEY (edge_id) REFERENCES edges(id)
);

INSERT OR IGNORE INTO paths_new (namespace, domain, path, edge_id, created_at)
SELECT '', domain, path, edge_id, created_at FROM paths;

DROP TABLE paths;
ALTER TABLE paths_new RENAME TO paths;
CREATE INDEX IF NOT EXISTS idx_paths_edge_id ON paths(edge_id);

DROP TABLE IF EXISTS search_documents_fts;

CREATE TABLE IF NOT EXISTS search_documents_new (
  namespace TEXT NOT NULL DEFAULT '',
  domain TEXT NOT NULL,
  path TEXT NOT NULL,
  node_uuid TEXT NOT NULL,
  memory_id INTEGER NOT NULL,
  uri TEXT NOT NULL,
  content TEXT NOT NULL,
  disclosure TEXT NULL,
  search_terms TEXT NOT NULL DEFAULT '',
  priority INTEGER NOT NULL DEFAULT 0,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (namespace, domain, path),
  FOREIGN KEY (node_uuid) REFERENCES nodes(uuid) ON DELETE CASCADE,
  FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE
);

INSERT OR IGNORE INTO search_documents_new
  (namespace, domain, path, node_uuid, memory_id, uri, content, disclosure, search_terms, priority, updated_at)
SELECT '', domain, path, node_uuid, memory_id, uri, content, disclosure, search_terms, priority, updated_at
FROM search_documents;

DROP TABLE search_documents;
ALTER TABLE search_documents_new RENAME TO search_documents;
CREATE INDEX IF NOT EXISTS idx_search_documents_node_uuid ON search_documents(node_uuid);
CREATE INDEX IF NOT EXISTS idx_search_documents_memory_id ON search_documents(memory_id);

CREATE VIRTUAL TABLE IF NOT EXISTS search_documents_fts USING fts5 (
  namespace UNINDEXED,
  domain UNINDEXED,
  path,
  node_uuid UNINDEXED,
  uri,
  content,
  disclosure,
  search_terms,
  tokenize = 'unicode61'
);

INSERT INTO search_documents_fts(namespace, domain, path, node_uuid, uri, content, disclosure, search_terms)
SELECT namespace, domain, path, node_uuid, uri, content, disclosure, search_terms
FROM search_documents;

CREATE TABLE IF NOT EXISTS glossary_keywords_new (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  keyword TEXT NOT NULL,
  node_uuid TEXT NOT NULL,
  namespace TEXT NOT NULL DEFAULT '',
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (node_uuid) REFERENCES nodes(uuid) ON DELETE CASCADE,
  CONSTRAINT uq_glossary_keyword_node UNIQUE (keyword, node_uuid, namespace)
);

INSERT OR IGNORE INTO glossary_keywords_new (id, keyword, node_uuid, namespace, created_at)
SELECT id, keyword, node_uuid, '', created_at FROM glossary_keywords;

DROP TABLE glossary_keywords;
ALTER TABLE glossary_keywords_new RENAME TO glossary_keywords;
CREATE INDEX IF NOT EXISTS idx_glossary_keywords_node_uuid ON glossary_keywords(node_uuid);
CREATE INDEX IF NOT EXISTS idx_glossary_keywords_keyword ON glossary_keywords(keyword);
"#;

const MIGRATION_0007_NAMESPACE_WRITE_ISOLATION: &str = r#"
CREATE TABLE IF NOT EXISTS memories_new (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  namespace TEXT NOT NULL DEFAULT '',
  node_uuid TEXT NOT NULL,
  content TEXT NOT NULL,
  deprecated BOOLEAN NOT NULL DEFAULT FALSE,
  migrated_to INTEGER NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (node_uuid) REFERENCES nodes(uuid)
);

INSERT INTO memories_new(id, namespace, node_uuid, content, deprecated, migrated_to, created_at)
SELECT id, '', node_uuid, content, deprecated, migrated_to, created_at
FROM memories;

DROP TABLE memories;
ALTER TABLE memories_new RENAME TO memories;
CREATE INDEX IF NOT EXISTS idx_memories_node_uuid ON memories(node_uuid);
CREATE INDEX IF NOT EXISTS idx_memories_node_uuid_deprecated ON memories(node_uuid, deprecated);
CREATE INDEX IF NOT EXISTS idx_memories_namespace_node_uuid ON memories(namespace, node_uuid);
CREATE INDEX IF NOT EXISTS idx_memories_namespace_node_uuid_deprecated ON memories(namespace, node_uuid, deprecated);

CREATE TABLE IF NOT EXISTS edges_new (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  namespace TEXT NOT NULL DEFAULT '',
  parent_uuid TEXT NOT NULL,
  child_uuid TEXT NOT NULL,
  name TEXT NOT NULL,
  priority INTEGER NOT NULL DEFAULT 0,
  disclosure TEXT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (parent_uuid) REFERENCES nodes(uuid),
  FOREIGN KEY (child_uuid) REFERENCES nodes(uuid),
  CONSTRAINT uq_edges_namespace_parent_child_name UNIQUE (namespace, parent_uuid, child_uuid, name)
);

INSERT INTO edges_new(id, namespace, parent_uuid, child_uuid, name, priority, disclosure, created_at)
SELECT id, '', parent_uuid, child_uuid, name, priority, disclosure, created_at
FROM edges;

DROP TABLE edges;
ALTER TABLE edges_new RENAME TO edges;
CREATE INDEX IF NOT EXISTS idx_edges_parent_uuid ON edges(parent_uuid);
CREATE INDEX IF NOT EXISTS idx_edges_child_uuid ON edges(child_uuid);
CREATE INDEX IF NOT EXISTS idx_edges_namespace_parent_uuid ON edges(namespace, parent_uuid);
CREATE INDEX IF NOT EXISTS idx_edges_namespace_child_uuid ON edges(namespace, child_uuid);

CREATE TABLE IF NOT EXISTS audit_log_new (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  namespace TEXT NOT NULL DEFAULT '',
  action TEXT NOT NULL,
  uri TEXT,
  node_uuid TEXT,
  details TEXT,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO audit_log_new(id, namespace, action, uri, node_uuid, details, created_at)
SELECT id, '', action, uri, node_uuid, details, created_at
FROM audit_log;

DROP TABLE audit_log;
ALTER TABLE audit_log_new RENAME TO audit_log;
CREATE INDEX IF NOT EXISTS idx_audit_log_action_created_at ON audit_log(action, created_at);
CREATE INDEX IF NOT EXISTS idx_audit_log_node_uuid_created_at ON audit_log(node_uuid, created_at);
CREATE INDEX IF NOT EXISTS idx_audit_log_namespace_created_at ON audit_log(namespace, created_at);
CREATE INDEX IF NOT EXISTS idx_audit_log_namespace_action_created_at ON audit_log(namespace, action, created_at);
"#;

const MIGRATIONS: [(&str, &str); 7] = [
    ("0001_core", MIGRATION_0001_CORE),
    ("0002_search", MIGRATION_0002_SEARCH),
    ("0003_search_fts", MIGRATION_0003_SEARCH_FTS),
    (
        "0004_edges_alias_name",
        MIGRATION_0004_EDGES_ALLOW_ALIAS_NAME,
    ),
    ("0005_audit_log", MIGRATION_0005_AUDIT_LOG),
    ("0006_namespace_compat", MIGRATION_0006_NAMESPACE_COMPAT),
    (
        "0007_namespace_write_isolation",
        MIGRATION_0007_NAMESPACE_WRITE_ISOLATION,
    ),
];

pub fn initialize_database(conn: &mut Connection, namespace: &str) -> Result<()> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS schema_migrations (
           version TEXT PRIMARY KEY,
           applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
         );",
    )?;

    for (version, sql) in MIGRATIONS {
        let applied = conn
            .query_row(
                "SELECT version FROM schema_migrations WHERE version = ?1",
                [version],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .is_some();
        if applied {
            continue;
        }
        apply_migration(conn, version, sql)?;
    }

    ensure_domain_root(conn, namespace, DEFAULT_DOMAIN)?;
    ensure_domain_root(conn, namespace, SYSTEM_DOMAIN)?;
    repair_search_documents_fts(conn, namespace)?;

    Ok(())
}

fn apply_migration(conn: &mut Connection, version: &str, sql: &str) -> Result<()> {
    if requires_foreign_keys_disabled(version) {
        conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
    }

    let migration_result = (|| {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_migrations(version) VALUES (?1)",
            [version],
        )?;
        tx.commit()?;
        Ok(())
    })();

    if requires_foreign_keys_disabled(version) {
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    }

    migration_result
}

fn requires_foreign_keys_disabled(version: &str) -> bool {
    matches!(
        version,
        "0004_edges_alias_name" | "0007_namespace_write_isolation"
    )
}

pub fn ensure_domain_root(conn: &Connection, namespace: &str, domain: &str) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO nodes(uuid) VALUES (?1)",
        [ROOT_NODE_UUID],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO paths(namespace, domain, path, edge_id) VALUES (?1, ?2, '', NULL)",
        params![namespace, domain],
    )?;
    Ok(())
}

fn repair_search_documents_fts(conn: &Connection, namespace: &str) -> Result<()> {
    let search_document_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM search_documents WHERE namespace = ?1",
        [namespace],
        |row| row.get(0),
    )?;
    let fts_document_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM search_documents_fts WHERE namespace = ?1",
        [namespace],
        |row| row.get(0),
    )?;

    if search_document_count == fts_document_count {
        return Ok(());
    }

    conn.execute(
        "DELETE FROM search_documents_fts WHERE namespace = ?1",
        [namespace],
    )?;
    conn.execute(
        "INSERT INTO search_documents_fts(namespace, domain, path, node_uuid, uri, content, disclosure, search_terms)
         SELECT namespace, domain, path, node_uuid, uri, content, disclosure, search_terms
         FROM search_documents
         WHERE namespace = ?1",
        [namespace],
    )?;

    Ok(())
}

pub fn active_memory_id_for_node(
    conn: &Connection,
    namespace: &str,
    node_uuid: &str,
) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT id
         FROM memories
         WHERE namespace = ?1 AND node_uuid = ?2 AND deprecated = FALSE
         ORDER BY id DESC
         LIMIT 1",
        params![namespace, node_uuid],
        |row| row.get::<_, i64>(0),
    )
    .optional()
    .map_err(Into::into)
}

pub fn mark_other_memories_deprecated(
    conn: &Connection,
    namespace: &str,
    node_uuid: &str,
    replacement_memory_id: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE memories
         SET deprecated = TRUE, migrated_to = ?3
         WHERE namespace = ?1 AND node_uuid = ?2 AND deprecated = FALSE AND id != ?3",
        params![namespace, node_uuid, replacement_memory_id],
    )?;
    Ok(())
}

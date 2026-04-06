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

const MIGRATIONS: [(&str, &str); 5] = [
    ("0001_core", MIGRATION_0001_CORE),
    ("0002_search", MIGRATION_0002_SEARCH),
    ("0003_search_fts", MIGRATION_0003_SEARCH_FTS),
    (
        "0004_edges_alias_name",
        MIGRATION_0004_EDGES_ALLOW_ALIAS_NAME,
    ),
    ("0005_audit_log", MIGRATION_0005_AUDIT_LOG),
];

pub fn initialize_database(conn: &mut Connection) -> Result<()> {
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

    ensure_domain_root(conn, DEFAULT_DOMAIN)?;
    ensure_domain_root(conn, SYSTEM_DOMAIN)?;

    Ok(())
}

fn apply_migration(conn: &mut Connection, version: &str, sql: &str) -> Result<()> {
    if version == "0004_edges_alias_name" {
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

    if version == "0004_edges_alias_name" {
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    }

    migration_result
}

pub fn ensure_domain_root(conn: &Connection, domain: &str) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO nodes(uuid) VALUES (?1)",
        [ROOT_NODE_UUID],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO paths(domain, path, edge_id) VALUES (?1, '', NULL)",
        [domain],
    )?;
    Ok(())
}

pub fn active_memory_id_for_node(conn: &Connection, node_uuid: &str) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM memories WHERE node_uuid = ?1 AND deprecated = FALSE ORDER BY id DESC LIMIT 1",
        [node_uuid],
        |row| row.get::<_, i64>(0),
    )
    .optional()
    .map_err(Into::into)
}

pub fn mark_other_memories_deprecated(
    conn: &Connection,
    node_uuid: &str,
    replacement_memory_id: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE memories
         SET deprecated = TRUE, migrated_to = ?2
         WHERE node_uuid = ?1 AND deprecated = FALSE AND id != ?2",
        params![node_uuid, replacement_memory_id],
    )?;
    Ok(())
}

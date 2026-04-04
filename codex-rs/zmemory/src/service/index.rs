use anyhow::Result;
use rusqlite::Connection;
use rusqlite::params;

pub(crate) fn rebuild_search_index(conn: &mut Connection) -> Result<i64> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM search_documents", [])?;
    tx.execute("DELETE FROM search_documents_fts", [])?;

    let rows = {
        let mut stmt = tx.prepare(
            "SELECT
                p.domain,
                p.path,
                e.child_uuid,
                m.id,
                m.content,
                e.disclosure,
                e.priority,
                COALESCE((
                    SELECT GROUP_CONCAT(keyword, ' ')
                    FROM glossary_keywords
                    WHERE node_uuid = e.child_uuid
                ), '')
             FROM paths p
             JOIN edges e ON e.id = p.edge_id
             JOIN memories m ON m.node_uuid = e.child_uuid AND m.deprecated = FALSE
             ORDER BY p.domain ASC, p.path ASC",
        )?;
        stmt.query_map([], |row| {
            let domain: String = row.get(0)?;
            let path: String = row.get(1)?;
            let node_uuid: String = row.get(2)?;
            let memory_id: i64 = row.get(3)?;
            let content: String = row.get(4)?;
            let disclosure: Option<String> = row.get(5)?;
            let priority: i64 = row.get(6)?;
            let keywords: String = row.get(7)?;
            Ok((
                domain, path, node_uuid, memory_id, content, disclosure, priority, keywords,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };

    for (domain, path, node_uuid, memory_id, content, disclosure, priority, keywords) in rows {
        let uri = format!("{domain}://{path}");
        let search_terms = build_search_terms(&domain, &path, &content, &keywords);
        tx.execute(
            "INSERT INTO search_documents(
                domain, path, node_uuid, memory_id, uri, content, disclosure, search_terms, priority
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                domain,
                path,
                node_uuid,
                memory_id,
                uri,
                content,
                disclosure,
                search_terms,
                priority
            ],
        )?;
        tx.execute(
            "INSERT INTO search_documents_fts(domain, path, uri, content, disclosure, search_terms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![domain, path, uri, content, disclosure, search_terms],
        )?;
    }
    tx.commit()?;

    conn.query_row("SELECT COUNT(*) FROM search_documents", [], |row| {
        row.get(0)
    })
    .map_err(Into::into)
}

fn build_search_terms(domain: &str, path: &str, content: &str, keywords: &str) -> String {
    let mut terms = vec![
        domain.to_string(),
        normalize_search_field(domain),
        normalize_search_field(path),
        normalize_search_field(&format!("{domain}://{path}")),
        normalize_search_field(content),
        ascii_search_tokens(content).join(" "),
    ];
    if !keywords.trim().is_empty() {
        terms.push(normalize_search_field(keywords.trim()));
    }
    terms.join(" ")
}

fn normalize_search_field(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            ':' | '/' | '.' | '-' => ' ',
            _ => ch.to_ascii_lowercase(),
        })
        .collect::<String>()
}

pub(super) fn normalize_search_query(query: &str) -> String {
    if query.chars().any(is_cjk_rune) {
        normalize_search_field(query)
    } else {
        let tokens = snippet_query_tokens(query);
        if tokens.is_empty() {
            normalize_search_field(query)
        } else {
            tokens.join(" ")
        }
    }
}

fn ascii_search_tokens(value: &str) -> Vec<String> {
    snippet_query_tokens(value)
        .into_iter()
        .filter(|token| {
            token
                .chars()
                .any(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        })
        .collect()
}

pub(super) fn snippet_query_tokens(query: &str) -> Vec<String> {
    let normalized: String = query
        .chars()
        .map(|ch| match ch {
            ':' | '/' | '.' | '-' => ' ',
            _ => ch,
        })
        .collect();
    let mut tokens = Vec::new();
    let mut ascii_run = String::new();
    let mut cjk_run = String::new();

    let flush_ascii = |tokens: &mut Vec<String>, ascii_run: &mut String| {
        if ascii_run.is_empty() {
            return;
        }
        tokens.push(ascii_run.to_lowercase());
        ascii_run.clear();
    };
    let flush_cjk = |tokens: &mut Vec<String>, cjk_run: &mut String| {
        if cjk_run.is_empty() {
            return;
        }
        tokens.extend(split_cjk_run(cjk_run));
        cjk_run.clear();
    };

    for ch in normalized.chars() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            flush_cjk(&mut tokens, &mut cjk_run);
            if should_split_ascii_snippet_run(&ascii_run, ch) {
                flush_ascii(&mut tokens, &mut ascii_run);
            }
            ascii_run.push(ch);
        } else if is_cjk_rune(ch) {
            flush_ascii(&mut tokens, &mut ascii_run);
            cjk_run.push(ch);
        } else {
            flush_ascii(&mut tokens, &mut ascii_run);
            flush_cjk(&mut tokens, &mut cjk_run);
        }
    }
    flush_ascii(&mut tokens, &mut ascii_run);
    flush_cjk(&mut tokens, &mut cjk_run);

    let mut seen = std::collections::BTreeSet::new();
    tokens
        .into_iter()
        .filter(|token| !token.is_empty() && seen.insert(token.clone()))
        .collect()
}

fn should_split_ascii_snippet_run(current: &str, next: char) -> bool {
    if current.is_empty() || !next.is_ascii_uppercase() {
        return false;
    }
    current
        .chars()
        .last()
        .is_some_and(|last| last.is_ascii_lowercase() || last.is_ascii_digit())
}

fn split_cjk_run(run: &str) -> Vec<String> {
    let chars = run.chars().collect::<Vec<_>>();
    if chars.len() <= 1 {
        return vec![run.to_string()];
    }
    let mut tokens = Vec::new();
    for width in [3_usize, 2, 1] {
        if chars.len() < width {
            continue;
        }
        for start in 0..=chars.len() - width {
            tokens.push(chars[start..start + width].iter().collect::<String>());
        }
    }
    tokens
}

fn is_cjk_rune(ch: char) -> bool {
    matches!(
        ch as u32,
        0x4E00..=0x9FFF
            | 0x3400..=0x4DBF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B820..=0x2CEAF
            | 0xF900..=0xFAFF
            | 0x2F800..=0x2FA1F
    )
}

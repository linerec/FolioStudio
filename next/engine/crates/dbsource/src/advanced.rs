//! The advanced tab's SQL: one `SELECT` or `WITH` statement (design 0, "고급 SQL"). It is checked here and then run inside a
//! read-only transaction (PostgreSQL `READ ONLY`, MySQL `START TRANSACTION READ ONLY`, SQLite `query_only`): the check gives a
//! clear refusal, the transaction is what makes writing impossible. SQL Server has no read-only transaction: there the check is the guard
//! and the transaction (READ COMMITTED) is always rolled back. Parameters are written `:name` and are bound.
use crate::error::{DbError, Result};
use crate::model::Driver;
use crate::params::Resolved;
use std::collections::BTreeMap;

/// Words that change data, the schema or the session: refused anywhere outside strings, quoted names and comments.
const WRITING: &[&str] = &[
    "insert", "update", "delete", "merge", "upsert", "drop", "alter", "create", "truncate", "grant", "revoke", "attach", "detach",
    "pragma", "copy", "call", "exec", "execute", "do", "load", "handler", "lock", "unlock", "set", "reset", "vacuum", "reindex",
    "commit", "rollback", "begin", "start", "savepoint", "release", "into", "outfile", "dumpfile", "listen", "notify", "prepare",
    "deallocate", "declare", "rename", "comment", "cluster", "refresh", "import", "install", "shutdown", "kill", "flush", "purge",
    // SQL Server: reading other servers or files, waiting, maintenance (EXEC, INTO and a second statement are refused above)
    "openrowset", "opendatasource", "openquery", "openxml", "bulk", "waitfor", "dbcc", "reconfigure", "backup", "restore", "writetext", "updatetext",
];

/// SQL Server's extended stored procedures (`xp_cmdshell`, …) are refused by their prefix wherever they are named.
const WRITING_PREFIXES: &[&str] = &["xp_"];

#[derive(Debug, Clone, PartialEq)]
enum Token { Word(String), Param(String), Semicolon, Other }

/// The tokens that matter (words, `:name` parameters, `;`) with their byte ranges; strings, quoted names, comments and
/// PostgreSQL dollar quotes are skipped.
fn tokens(sql: &str) -> Result<Vec<(Token, std::ops::Range<usize>)>> {
    let b = sql.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let unterminated = |what: &str| DbError::invalid(format!("the SQL has an unterminated {what}"));
    while i < b.len() {
        let c = b[i];
        match c {
            b'\'' | b'"' | b'`' => {
                let q = c;
                i += 1;
                loop {
                    if i >= b.len() { return Err(unterminated("quote")); }
                    if b[i] == b'\\' && q == b'\'' { i += 2; continue; }
                    if b[i] == q { if b.get(i + 1) == Some(&q) { i += 2; continue; } i += 1; break; }
                    i += 1;
                }
            }
            b'[' => { // SQL Server style quoted name
                match sql[i..].find(']') { Some(k) => i += k + 1, None => return Err(unterminated("[name]")) }
            }
            b'-' if b.get(i + 1) == Some(&b'-') => { i = sql[i..].find('\n').map_or(b.len(), |k| i + k + 1); }
            b'#' => { i = sql[i..].find('\n').map_or(b.len(), |k| i + k + 1); }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                match sql[i + 2..].find("*/") { Some(k) => i += k + 4, None => return Err(unterminated("comment")) }
            }
            b'$' => {
                // $tag$ … $tag$ (PostgreSQL); a lone $1 is a positional parameter, refused: use :name
                let rest = &sql[i + 1..];
                let tag_len = rest.find('$').filter(|&k| rest[..k].chars().all(|ch| ch.is_alphanumeric() || ch == '_') && !rest[..k].starts_with(|ch: char| ch.is_ascii_digit()));
                match tag_len {
                    Some(k) => {
                        let tag = &sql[i..i + k + 2];
                        let body = i + k + 2;
                        match sql[body..].find(tag) { Some(e) => i = body + e + tag.len(), None => return Err(unterminated("dollar quote")) }
                    }
                    None => return Err(DbError::invalid("write parameters as :name, not $1")),
                }
            }
            b':' if b.get(i + 1) == Some(&b':') => { out.push((Token::Other, i..i + 2)); i += 2; }
            b':' => {
                let start = i;
                let name: String = sql[i + 1..].chars().take_while(|ch| ch.is_alphanumeric() || *ch == '_').collect();
                if name.is_empty() { out.push((Token::Other, i..i + 1)); i += 1; } else { i += 1 + name.len(); out.push((Token::Param(name), start..i)); }
            }
            b'?' => return Err(DbError::invalid("write parameters as :name, not ?")),
            // `@@VERSION` and the like are read; `@P1` or `@x` (SQL Server parameters, variables) are not parameters here
            b'@' if b.get(i + 1) == Some(&b'@') => { out.push((Token::Other, i..i + 2)); i += 2; }
            b'@' if b.get(i + 1).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c >= 0x80) => return Err(DbError::invalid("write parameters as :name, not @P1")),
            b';' => { out.push((Token::Semicolon, i..i + 1)); i += 1; }
            _ if c.is_ascii_whitespace() => i += 1,
            _ if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 => {
                let start = i;
                let word: String = sql[i..].chars().take_while(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '$').collect();
                i += word.len();
                out.push((Token::Word(word.to_lowercase()), start..i));
            }
            _ => { let ch = sql[i..].chars().next().map_or(1, char::len_utf8); out.push((Token::Other, i..i + ch)); i += ch; }
        }
    }
    Ok(out)
}

/// The statement checked: it must be one `SELECT` or `WITH` (a trailing `;` is allowed) and name nothing that writes.
pub fn check(sql: &str) -> Result<()> {
    if sql.len() > 100_000 { return Err(DbError::invalid("the SQL is longer than 100000 characters")); }
    let toks = tokens(sql)?;
    let first = toks.first().map(|(t, r)| match t { Token::Word(w) => w.as_str(), _ => &sql[r.clone()] });
    let first_word = toks.iter().find_map(|(t, _)| if let Token::Word(w) = t { Some(w.as_str()) } else { None });
    if !(matches!(first, Some("select" | "with")) || (first == Some("(") && first_word == Some("select"))) {
        return Err(DbError::invalid("only one SELECT or WITH statement is allowed"));
    }
    let semis: Vec<usize> = toks.iter().enumerate().filter(|(_, (t, _))| *t == Token::Semicolon).map(|(i, _)| i).collect();
    if semis.iter().any(|&k| toks[k + 1..].iter().any(|(t, _)| *t != Token::Semicolon)) {
        return Err(DbError::invalid("only one statement is allowed"));
    }
    for (k, (t, _)) in toks.iter().enumerate() {
        if let Token::Word(w) = t {
            // REPLACE( is a text function; REPLACE INTO is caught by INTO
            // `SET` inside a type (`CHARACTER SET`) is refused too: rare in a SELECT, and the transaction is read-only anyway
            if let Some(p) = WRITING_PREFIXES.iter().find(|p| w.starts_with(**p)) {
                return Err(DbError::invalid(format!("the SQL may only read: {} is not allowed", p.to_uppercase())));
            }
            if WRITING.contains(&w.as_str()) {
                let next_is_paren = matches!(toks.get(k + 1), Some((Token::Other, r)) if &sql[r.clone()] == "(");
                if !(next_is_paren && matches!(w.as_str(), "load" | "lock" | "comment" | "refresh")) {
                    return Err(DbError::invalid(format!("the SQL may only read: {} is not allowed", w.to_uppercase())));
                }
            }
        }
    }
    Ok(())
}

/// The statement with each `:name` replaced by the kind's placeholder, and the values in order (a name used twice is bound twice).
pub fn bind(sql: &str, driver: Driver, params: &BTreeMap<String, Resolved>) -> Result<(String, Vec<String>)> {
    check(sql)?;
    let toks = tokens(sql)?;
    let mut out = String::with_capacity(sql.len());
    let mut binds = Vec::new();
    let mut last = 0;
    for (t, r) in toks {
        let Token::Param(name) = t else { continue };
        let value = params.get(&name).ok_or_else(|| DbError::invalid(format!("the SQL uses :{name}, which is not a parameter")))?;
        out.push_str(&sql[last..r.start]);
        let text = match value {
            Resolved::Text(s) | Resolved::Number(s) => s.clone(),
            Resolved::Date(d) => d.to_string(),
            Resolved::Month(m) => m.first().to_string(),
            Resolved::List(_) => return Err(DbError::invalid(format!(":{name} is a list; lists work in the condition builder"))),
        };
        binds.push(text);
        out.push_str(&crate::sql::placeholder(driver, binds.len()));
        last = r.end;
    }
    out.push_str(&sql[last..]);
    let trimmed = out.trim_end().trim_end_matches(';').trim_end().to_owned();
    Ok((trimmed, binds))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{Date, Month};

    #[test]
    fn reading_statements_pass() {
        for ok in ["SELECT * FROM ads", "  with x as (select 1) select * from x;", "(SELECT 1)", "select 'delete' as \"update\" -- drop\n from t",
            "select /* insert */ id::text, replace(title, 'a', 'b') from t where d = :월", "SELECT $$ drop $$ AS x", "select `into` from t"] {
            assert!(check(ok).is_ok(), "{ok}: {:?}", check(ok));
        }
    }

    #[test]
    fn writing_statements_are_refused() {
        for bad in ["DELETE FROM ads", "select 1; drop table ads", "WITH d AS (DELETE FROM ads RETURNING *) SELECT * FROM d", "select * into newt from ads",
            "select * from ads for update", "UPDATE ads SET a=1", "PRAGMA writable_schema=1", "select 1 into outfile '/tmp/x'", "insert into t values (1)",
            "select * from t where a = ?", "select * from t where a = $1", "select 'unterminated", "/* x", "SET ROLE admin", "COPY t TO PROGRAM 'x'", "CALL p()"] {
            assert_eq!(check(bad).unwrap_err().kind, crate::Kind::Invalid, "{bad}");
        }
    }

    #[test]
    fn sql_server_reading_passes_and_its_dangerous_statements_are_refused() {
        for ok in ["SELECT TOP (10) [id], N'한글' AS [into] FROM [dbo].[ads] WITH (NOLOCK) ORDER BY [id] DESC",
            "WITH c AS (SELECT id FROM ads) SELECT * FROM c ORDER BY id OFFSET 0 ROWS FETCH NEXT 5 ROWS ONLY;",
            "select [exec], [xp_x] from t where name = N'EXEC xp_cmdshell' -- exec\n", "SELECT @@VERSION", "SELECT CONVERT(NVARCHAR(30), d, 121) FROM t FOR JSON PATH"] {
            assert!(check(ok).is_ok(), "{ok}: {:?}", check(ok));
        }
        for bad in ["EXEC xp_cmdshell 'dir'", "EXECUTE sp_who", "select 1 exec('drop table ads')", "SELECT * FROM OPENROWSET('SQLNCLI', 'Server=x;', 'SELECT 1')",
            "select * from opendatasource('SQLNCLI', 'Data Source=x').db.dbo.t", "SELECT * FROM OPENQUERY(other, 'SELECT 1')", "select * into #copy from ads",
            "SELECT * INTO dbo.copy FROM ads", "select 1; select 2", "SELECT 1; DROP TABLE ads", "select master.dbo.xp_cmdshell('dir')", "WAITFOR DELAY '00:00:10'",
            "select 1 waitfor delay '0:0:5'", "DBCC CHECKDB", "BULK INSERT ads FROM 'c:/x.csv'", "select * from ads; exec sp_configure", "BACKUP DATABASE m TO DISK = 'x'",
            "SELECT * FROM OPENXML(@h, '/r')", "select * from t where a = @P1"] {
            assert_eq!(check(bad).unwrap_err().kind, crate::Kind::Invalid, "{bad}");
        }
    }

    #[test]
    fn named_parameters_are_bound_per_kind() {
        let params = BTreeMap::from([("월".to_string(), Resolved::Month(Month { y: 2026, m: 3 })), ("min".to_string(), Resolved::Number("10".into())),
            ("d".to_string(), Resolved::Date(Date { y: 2026, m: 3, d: 9 }))]);
        let sql = "select * from t where m >= :월 and price > :min and d = :d and x::text = ':notparam' and y = :min;";
        let (pg, binds) = bind(sql, Driver::Postgres, &params).unwrap();
        assert_eq!(pg, "select * from t where m >= $1 and price > $2 and d = $3 and x::text = ':notparam' and y = $4");
        assert_eq!(binds, ["2026-03-01", "10", "2026-03-09", "10"]);
        let (ms, _) = bind(sql, Driver::Sqlserver, &params).unwrap();
        assert_eq!(ms, "select * from t where m >= @P1 and price > @P2 and d = @P3 and x::text = ':notparam' and y = @P4");
        let (my, _) = bind(sql, Driver::Mysql, &params).unwrap();
        assert_eq!(my, "select * from t where m >= ? and price > ? and d = ? and x::text = ':notparam' and y = ?");
        assert!(bind("select :없음", Driver::Sqlite, &params).is_err());
    }
}

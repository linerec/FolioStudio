//! Data Merge from a database: the host side (dbmerge DESIGN 2). The connections live in the app's user settings
//! (`settings.json` key `dbConnections`, never the project: principle 1 rule 8) and their passwords in the OS credential store
//! (`folio_dbsource::credentials`); a connection whose password was not saved is unlocked for this run of the engine with
//! `dbUnlockConnection` (the password lives in this process's memory only; `dbLockConnection` forgets it). The drivers look for a
//! password in this order: the session, the credential store; neither: `needsPassword`. No reply, log or error carries a password,
//! and no edit command takes one.
//!
//! Queries: `dbConnections`, `dbSaveConnection`, `dbRemoveConnection`, `dbUnlockConnection`, `dbLockConnection`, `dbTestConnection`,
//! `dbTables`, `dbColumns`, `dbPreview`, `dataMergeSourceStatus`. Edits: `dataMergeSelectDatabase`, `dataMergeRefresh` (`prepare` runs the query before the edit, writes
//! the snapshot CSV into the project as `assets/<document id>/datamerge/<sha256>.csv` and gives the edit its reference).
use crate::{Session, edit::EditState, place::Prepared};
use folio_dbsource::model::{Connection, Driver, Endpoint, Query, TableRef};
use folio_dbsource::params::Date;
use folio_dbsource::{DbError, Kind, Secret, credentials};
use folio_edit::{Command, commands::RegisteredCommand};
use folio_project::format::{self, RefKind, Reference};
use serde_json::{Map, Value, json};

const KEY: &str = "dbConnections";

fn refuse(e: DbError) -> Value {
    let mut v = json!({"ok": false, "errorKind": e.kind, "error": e.message});
    if let Some(fp) = e.fingerprint { v["fingerprint"] = json!(fp); }
    v
}

fn settings() -> Result<Vec<Connection>, DbError> {
    let all = crate::user_settings::update(|_, _| Ok(())).map_err(|e| DbError::new(Kind::Invalid, e))?;
    Ok(all.get(KEY).and_then(Value::as_array).map(|a| a.iter().filter_map(|c| serde_json::from_value(c.clone()).ok()).collect()).unwrap_or_default())
}

fn store(change: impl FnOnce(&mut Vec<Connection>)) -> Result<(), DbError> {
    crate::user_settings::update(|s, _| {
        let mut list: Vec<Connection> = s.get(KEY).and_then(Value::as_array).map(|a| a.iter().filter_map(|c| serde_json::from_value(c.clone()).ok()).collect()).unwrap_or_default();
        change(&mut list);
        s.insert(KEY.into(), serde_json::to_value(&list).map_err(|e| e.to_string())?);
        Ok(())
    }).map(|_| ()).map_err(|e| DbError::new(Kind::Invalid, e))
}

fn listed(c: &Connection) -> Value {
    let mut v = serde_json::to_value(c).unwrap_or(Value::Null);
    v["hasPassword"] = json!(credentials::load(&c.name).ok().flatten().is_some());
    v["unlocked"] = json!(credentials::session_unlocked(&c.name));
    v
}

/// `dbConnections` → `{connections:[{name, driver, host, port, database, user, tls, hasPassword, unlocked}], credentialStore}`
/// (`unlocked`: a password is kept for this session).
pub fn connections() -> Value {
    match settings() {
        Ok(list) => json!({"connections": list.iter().map(listed).collect::<Vec<_>>(), "credentialStore": credentials::available()}),
        Err(e) => refuse(e),
    }
}

fn connection_of(req: &Value) -> Result<Connection, DbError> {
    let mut v = req.clone();
    if let Some(o) = v.as_object_mut() { for k in ["cmd", "password", "savePassword", "origin", "id"] { o.remove(k); } }
    let c: Connection = serde_json::from_value(v).map_err(|e| DbError::invalid(format!("connection settings: {e}")))?;
    c.validate()?;
    Ok(c)
}

/// `dbSaveConnection {name, driver, host, port, instance?, database, user, password?, savePassword, tls}` → `{ok}` (`instance`: SQL Server).
pub fn save_connection(req: &Value) -> Value {
    let run = || -> Result<Value, DbError> {
        let c = connection_of(req)?;
        let save = req["savePassword"].as_bool().unwrap_or(false);
        match req["password"].as_str() {
            Some(p) if save => credentials::save(&c.name, &Secret::new(p))?,
            _ if !save => credentials::remove(&c.name)?,
            _ => {}
        }
        let name = c.name.clone();
        // the session password belongs to the server it was typed for: another host, port, instance, database or user forgets it
        if settings()?.iter().any(|o| o.name == c.name && (o.driver, &o.host, o.port, &o.instance, &o.database, &o.user) != (c.driver, &c.host, c.port, &c.instance, &c.database, &c.user)) {
            credentials::session_clear(&c.name);
        }
        store(move |list| match list.iter_mut().find(|o| o.name == name) { Some(o) => *o = c, None => list.push(c) })?;
        Ok(json!({"ok": true}))
    };
    run().unwrap_or_else(refuse)
}

/// `dbRemoveConnection {name}`: the settings and the saved password go.
pub fn remove_connection(req: &Value) -> Value {
    let run = || -> Result<Value, DbError> {
        let name = req["name"].as_str().ok_or_else(|| DbError::invalid("name required"))?.to_owned();
        credentials::session_clear(&name);
        credentials::remove(&name)?;
        store(|list| list.retain(|c| c.name != name))?;
        Ok(json!({"ok": true}))
    };
    run().unwrap_or_else(refuse)
}

/// `dbUnlockConnection {name, password, save?}` → `{ok, unlocked, saved}`: the password is tried against the server (a connection is made) and, only
/// when it is right, kept in this engine's memory for the session; with `save` it also goes to the OS credential store. A wrong password is
/// not kept (`errorKind` `auth`). The password is in this request only: no edit command, reply, log or file carries it.
pub fn unlock_connection(req: &Value, session: Option<&Session>) -> Value {
    let run = || -> Result<Value, DbError> {
        let name = req["name"].as_str().ok_or_else(|| DbError::invalid("name required"))?;
        let given = req["password"].as_str().filter(|p| !p.is_empty()).ok_or_else(|| DbError::invalid("password required"))?;
        let saved = settings()?.into_iter().find(|c| c.name == name).ok_or_else(|| DbError::new(Kind::NotFound, format!("the connection '{name}' is not on this computer")))?;
        if saved.driver == Driver::Sqlite { return Ok(json!({"ok": true, "unlocked": false, "saved": false, "note": "a SQLite file has no password"})); }
        let ep = endpoint(&json!({"connection": name, "password": given}), session, true)?;
        folio_dbsource::run::test(&ep)?;
        credentials::session_set(name, Secret::new(given));
        let save = req["save"].as_bool().unwrap_or(false);
        if save {
            // the password is right and kept for the session even when the credential store refuses it
            if let Err(e) = credentials::save(name, &Secret::new(given)) { let mut v = refuse(e); v["unlocked"] = json!(true); return Ok(v); }
        }
        Ok(json!({"ok": true, "unlocked": true, "saved": save}))
    };
    run().unwrap_or_else(refuse)
}

/// `dbLockConnection {name}` → `{ok}`: forget the session password of a connection (the credential store is not touched).
pub fn lock_connection(req: &Value) -> Value {
    let Some(name) = req["name"].as_str() else { return refuse(DbError::invalid("name required")) };
    credentials::session_clear(name);
    json!({"ok": true})
}

/// `dbCapabilities` → `{platform, credentialStore:{available, kind, reason?, message?}, sessionPasswords, tlsVerify:{available, reason?}}`:
/// what this device can do (dbplatform-c1). The screen greys "save the password" with the reason when the store is not available
/// (Android: the session only), and an LLM learns the same without trying.
pub fn capabilities() -> Value { folio_dbsource::capabilities() }

/// `dbCredentialStoreSelfTest` → `{ok, store, steps:[{step, ok, errorKind?, error?}], ms}`: keep, read and remove a throwaway entry in
/// this computer's credential store (Help > Diagnostics; a Mac's keychain, a Linux desktop's Secret Service). No connection is touched
/// and no value is returned.
pub fn credential_store_self_test() -> Value { serde_json::to_value(credentials::self_test()).unwrap_or(Value::Null) }

/// The connection a request names (`connection`/`name`: a saved one) or carries (`connection` as settings, unsaved), with its
/// password and, for SQLite, the file inside the open project. The password is the one the request carries only when `typed` (a test
/// of settings not saved yet, `dbUnlockConnection`); otherwise it is the session's, then the credential store's (`credentials::resolve`).
fn endpoint(req: &Value, session: Option<&Session>, typed: bool) -> Result<Endpoint, DbError> {
    let named = req["connection"].as_str().or_else(|| req["name"].as_str().filter(|_| req.get("driver").is_none()));
    let connection = match (&req["connection"], named) {
        (_, Some(name)) => settings()?.into_iter().find(|c| c.name == name)
            .ok_or_else(|| DbError::new(Kind::NotFound, format!("the connection '{name}' is not on this computer")))?,
        (Value::Object(_), None) => connection_of(&req["connection"])?,
        _ => connection_of(req)?,
    };
    let password = match req["password"].as_str().filter(|p| typed && !p.is_empty()) {
        Some(p) => Some(Secret::new(p)),
        None => credentials::resolve(&connection.name),
    };
    let file = if connection.driver == Driver::Sqlite {
        let project = &session.and_then(|s| s.folio.as_ref()).ok_or_else(|| DbError::invalid("a SQLite file is opened from the open project"))?.opened.project;
        let path = folio_project::paths::checked_project_path(&project.root, &connection.database).map_err(DbError::invalid)?;
        Some(path)
    } else { None };
    Ok(Endpoint { connection, password, file })
}

fn today(req: &Value) -> Result<Date, DbError> {
    match req["today"].as_str() {
        Some(t) => Date::parse(t).ok_or_else(|| DbError::invalid("today is YYYY-MM-DD")),
        None => Ok(Date::today_utc()),
    }
}

/// `dbTestConnection {connection | name | settings…, password?}` → `{ok, serverVersion, ms}` or `{ok:false, errorKind, error, fingerprint?}`.
pub fn test_connection(req: &Value, session: Option<&Session>) -> Value {
    match endpoint(req, session, true).and_then(|ep| folio_dbsource::run::test(&ep)) {
        Ok(t) => json!({"ok": true, "serverVersion": t.server_version, "ms": t.ms}),
        Err(e) => refuse(e),
    }
}

/// `dbTables {connection}` → `{tables:[{schema, name, kind}]}`.
pub fn tables(req: &Value, session: Option<&Session>) -> Value {
    match endpoint(req, session, false).and_then(|ep| folio_dbsource::run::tables(&ep)) {
        Ok(t) => json!({"tables": t}),
        Err(e) => refuse(e),
    }
}

/// `dbColumns {connection, table:{schema, name}}` → `{columns:[{name, type, nullable}]}`.
pub fn columns(req: &Value, session: Option<&Session>) -> Value {
    let run = || -> Result<Value, DbError> {
        let table: TableRef = serde_json::from_value(req["table"].clone()).map_err(|e| DbError::invalid(format!("table: {e}")))?;
        Ok(json!({"columns": folio_dbsource::run::columns(&endpoint(req, session, false)?, &table)?}))
    };
    run().unwrap_or_else(refuse)
}

fn query_of(v: &Value) -> Result<Query, DbError> { serde_json::from_value(v.clone()).map_err(|e| DbError::invalid(format!("query: {e}"))) }
fn given(v: &Value) -> Map<String, Value> { v.as_object().cloned().unwrap_or_default() }

/// `dbPreview {query, paramValues?, limit?, today?}` → `{fields, records, count, sql, params, ms, paramValues}`.
pub fn preview(req: &Value, session: Option<&Session>) -> Value {
    let run = || -> Result<Value, DbError> {
        let query = query_of(&req["query"])?;
        let ep = endpoint(&json!({"connection": query.connection}), session, false)?;
        let limit = req["limit"].as_u64().map_or(folio_dbsource::PREVIEW_ROWS, |n| n.clamp(1, 1000) as usize);
        let f = folio_dbsource::run::fetch(&ep, &query, &given(&req["paramValues"]), today(req)?, Some(limit))?;
        let source = folio_dbsource::snapshot::source(&folio_dbsource::snapshot::csv_bytes(&f.table)).ok();
        Ok(json!({"fields": source.as_ref().map(|s| json!(s.fields)).unwrap_or(json!([])), "columns": f.table.columns,
            "records": f.table.rows, "count": f.count, "sql": f.sql, "params": f.binds, "ms": f.ms, "paramValues": f.param_values}))
    };
    run().unwrap_or_else(refuse)
}

/// The rows of the query, as the snapshot and the state the document label keeps.
struct Fetched { bytes: Vec<u8>, state: Map<String, Value>, rows: usize }

fn fetch(query: &Value, params: &Option<std::collections::BTreeMap<String, Value>>, day: &Option<String>, session: &Session) -> Result<Fetched, DbError> {
    let q = query_of(query)?;
    let ep = endpoint(&json!({"connection": q.connection}), Some(session), false)?;
    let today = today(&json!({"today": day}))?;
    let given: Map<String, Value> = params.clone().map(|m| m.into_iter().collect()).unwrap_or_default();
    let f = folio_dbsource::run::fetch(&ep, &q, &given, today, None)?;
    f.table.check_limits()?;
    let bytes = folio_dbsource::snapshot::csv_bytes(&f.table);
    let source = folio_dbsource::snapshot::source(&bytes)?;
    // the document keeps the hint (kind, host, port, database), never the user or the password
    let mut query = query.clone();
    query["hint"] = json!(ep.connection.hint());
    let mut state = Map::new();
    state.insert("query".into(), query);
    state.insert("paramValues".into(), Value::Object(f.param_values));
    state.insert("fetchedAt".into(), json!(folio_project::util::now_rfc3339()));
    state.insert("rows".into(), json!(source.records.len()));
    state.insert("fields".into(), json!(source.fields));
    state.insert("sha256".into(), json!(folio_dbsource::snapshot::sha256(&bytes)));
    Ok(Fetched { rows: source.records.len(), bytes, state })
}

/// The snapshot written into the project (content-addressed, through a temporary file) and its reference.
fn snapshot(bytes: &[u8], connection: &str, cur: &Session, prepared: &mut Prepared) -> Result<String, String> {
    let folio = cur.folio.as_ref().ok_or("A database source can only be selected in a project document")?;
    let root = &folio.opened.project.root;
    let sha = folio_project::hash::sha256_bytes(bytes);
    let rel = folio_project::RelPath::parse(&format!("assets/{}/datamerge/{sha}.csv", folio.opened.manifest.id)).map_err(|e| e.to_string())?;
    let dest = rel.to_native(root);
    if !dest.is_file() {
        std::fs::create_dir_all(dest.parent().ok_or("no folder")?).map_err(|e| e.to_string())?;
        folio_project::atomic::write_bytes(&dest, bytes).map_err(|e| e.to_string())?;
        prepared.created.push(dest);
    }
    let r = Reference { id: format!("r{}", folio_project::util::short_id()), kind: RefKind::File, path: Some(rel), sha256: Some(sha),
        bytes: Some(bytes.len() as u64), hint: None, original_name: None, status: Some("found".into()), font: None,
        extra: [("database".to_string(), json!(connection))].into_iter().collect() };
    let id = r.id.clone();
    prepared.refs.push(r);
    Ok(format!("{}{id}", format::PATH_REF_SCHEME))
}

/// Before an edit with `dataMergeSelectDatabase` / `dataMergeRefresh`: the query runs, the snapshot goes into the project and the
/// command gets its reference and the label state. The errors are the reasons it is refused (`errorKind` as the queries give it).
pub(crate) fn prepare(commands: &mut [Command], cur: &Session, es: &EditState, prepared: &mut Prepared) -> Result<(), Value> {
    let refusal = |e: DbError| { let mut v = refuse(e); v["code"] = json!("source"); v };
    for command in commands {
        let (query, params, day, source, file_type, state, before) = match command {
            Command::Registered(RegisteredCommand::DataMergeSelectDatabase { query, param_values, today, source, file_type, state }) => {
                if source.is_some() { continue; }
                (query.clone(), param_values.clone(), today.clone(), source, file_type, state, None)
            }
            Command::Registered(RegisteredCommand::DataMergeRefresh { param_values, today, source, file_type, state }) => {
                if source.is_some() { continue; }
                let before = es.editor.data_merge_database().ok_or_else(|| refusal(DbError::invalid("the document's data source is not a database")))?;
                // the remembered values unless new ones are given
                let params = param_values.clone().or_else(|| before.get("paramValues").and_then(|v| serde_json::from_value(v.clone()).ok()));
                (before["query"].clone(), params, today.clone(), source, file_type, state, Some(before))
            }
            _ => continue,
        };
        let fetched = fetch(&query, &params, &day, cur).map_err(refusal)?;
        let mut st = fetched.state;
        if let Some(before) = before {
            // what changed, record by record in order (no key is known): rows added at the end, gone from the end, changed in place
            let _ = before;
            let old = current_snapshot(cur, es).and_then(|(_, b)| folio_dbsource::snapshot::source(&b).ok());
            let new = folio_dbsource::snapshot::source(&fetched.bytes).map_err(refusal)?;
            let old = old.map(|s| s.records).unwrap_or_default();
            let same = old.len().min(new.records.len());
            let changed = (0..same).filter(|&i| old[i] != new.records[i]).count();
            st.insert("lastRefresh".into(), json!({"rows": fetched.rows, "added": new.records.len().saturating_sub(old.len()),
                "removed": old.len().saturating_sub(new.records.len()), "changed": changed}));
        }
        let connection = query["connection"].as_str().unwrap_or_default().to_owned();
        let reference = snapshot(&fetched.bytes, &connection, cur, prepared).map_err(|e| json!({"ok": false, "code": "source", "error": e}))?;
        *source = Some(reference);
        *file_type = Some(String::new());
        *state = Some(Value::Object(st));
    }
    Ok(())
}

/// The document's data source file (its `DataMerge` reference) and its bytes.
fn current_snapshot(cur: &Session, es: &EditState) -> Option<(Reference, Vec<u8>)> {
    let f = cur.folio.as_ref()?;
    let (file, _) = es.editor.data_source()?;
    let id = file.strip_prefix(format::PATH_REF_SCHEME)?;
    let r = f.opened.manifest.references.iter().chain(es.graphics.iter().flat_map(|g| g.references.iter())).find(|r| r.id == id)?;
    let bytes = std::fs::read(r.path.as_ref()?.to_native(&f.opened.project.root)).ok()?;
    Some((r.clone(), bytes))
}

/// `dataMergeSourceStatus {check?, today?}` → `{kind:"file"|"database"|"none", connection, connectionPresent, fetchedAt, rows,
/// fields, lastRefresh, changed?}`; `check: true` runs the query again and compares the result with the snapshot's hash.
pub fn source_status(req: &Value, session: Option<&Session>) -> Result<Value, String> {
    let s = session.ok_or("Open a project document")?;
    let edit = s.edit.as_ref().ok_or("Open a project document")?;
    let (current, snapshot, database) = {
        let es = edit.lock().expect("shared state must remain unpoisoned after each operation");
        (es.editor.data_source().map(|(file, _)| file), current_snapshot(s, &es), es.editor.data_merge_database())
    };
    let Some(current) = current else { return Ok(json!({"kind": "none"})) };
    // the source is the database's when it is the snapshot the label tells (by content: it survives an IDML round trip)
    let sha = snapshot.map(|(_, b)| folio_dbsource::snapshot::sha256(&b));
    let Some(db) = database.filter(|d| sha.is_some() && d["sha256"].as_str() == sha.as_deref()) else { return Ok(json!({"kind": "file", "source": current})) };
    let connection = db["query"]["connection"].as_str().unwrap_or_default().to_owned();
    let present = settings().map(|l| l.iter().any(|c| c.name == connection)).unwrap_or(false);
    let mut reply = json!({"kind": "database", "connection": connection, "connectionPresent": present, "hint": db["query"]["hint"],
        "fetchedAt": db["fetchedAt"], "rows": db["rows"], "fields": db["fields"], "paramValues": db["paramValues"], "lastRefresh": db["lastRefresh"],
        "refresh": db["query"]["refresh"], "query": db["query"]});
    if req["check"].as_bool() == Some(true) {
        let params: Option<std::collections::BTreeMap<String, Value>> = serde_json::from_value(db["paramValues"].clone()).ok();
        match fetch(&db["query"], &params, &req["today"].as_str().map(str::to_owned), s) {
            Ok(f) => reply["changed"] = json!(f.state.get("sha256") != Some(&db["sha256"])),
            Err(e) => { reply["checkError"] = json!({"errorKind": e.kind, "error": e.message}); }
        }
    }
    Ok(reply)
}

/// Preflight: an info finding when the document's data source comes from a database connection this computer does not have
/// (design 3: the panel and the preflight say so; opening, printing, PDF and packages use the snapshot).
pub fn preflight_note(view: &mut Value, s: &Session) {
    let Ok(status) = source_status(&json!({}), Some(s)) else { return };
    if status["kind"] != "database" || status["connectionPresent"] == true { return; }
    let name = status["connection"].as_str().unwrap_or_default();
    let finding = json!({"rule": "dataMergeConnection", "severity": "info", "page": null, "pageName": null, "item": null,
        "message": format!("데이터 병합의 데이터베이스 연결 '{name}' 이(가) 이 컴퓨터에 없습니다. 마지막으로 가져온 스냅숏으로 병합·인쇄합니다."),
        "messageId": "preflight.datamerge.connection", "messageArgs": {"name": name}, "bounds": null});
    if let Some(list) = view["findings"].as_array_mut() { list.push(finding); }
    if let Some(n) = view["summary"]["infos"].as_u64() { view["summary"]["infos"] = json!(n + 1); }
    if let Some(by) = view["summary"]["byRule"].as_object_mut() { by.insert("dataMergeConnection".into(), json!(1)); }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_with_test_store(name: &str) -> bool {
        const CHILD: &str = "FOLIO_DBMERGE_TEST_CHILD";
        if std::env::var(CHILD).as_deref() == Ok(name) { return true; }
        let dir = std::env::temp_dir().join(format!("folio-dbmerge-{}-{name}", std::process::id()));
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact").arg(format!("dbmerge::tests::{name}")).arg("--nocapture")
            .env(CHILD, name)
            .env("FOLIO_USER_DATA", &dir)
            .env("FOLIO_TEST_CREDENTIALS", "memory")
            .output().unwrap();
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        false
    }

    #[test]
    fn passwords_never_come_back() {
        if !run_with_test_store("passwords_never_come_back") { return; }
        let dir = std::path::PathBuf::from(std::env::var_os("FOLIO_USER_DATA").unwrap());
        let secret = "S3cret-비밀-'\"";
        let saved = save_connection(&json!({"cmd": "dbSaveConnection", "name": "잡지DB", "driver": "postgres", "host": "127.0.0.1", "port": 1,
            "database": "magazine", "user": "folio", "password": secret, "savePassword": true, "tls": {"mode": "disable"}}));
        assert_eq!(saved["ok"], true, "{saved}");
        let list = connections();
        assert_eq!(list["connections"][0]["hasPassword"], true, "{list}");
        // a connection that cannot be reached: the error is structured and has no password in it
        let t = test_connection(&json!({"connection": "잡지DB"}), None);
        assert_eq!(t["ok"], false);
        assert!(matches!(t["errorKind"].as_str(), Some("network" | "timeout")), "{t}");
        let p = preview(&json!({"query": {"connection": "잡지DB", "table": {"name": "ads"}}}), None);
        assert_eq!(p["ok"], false);
        for reply in [&saved, &list, &t, &p] { assert!(!reply.to_string().contains(secret), "{reply}"); }
        let file = std::fs::read_to_string(dir.join("settings.json")).unwrap();
        assert!(!file.contains(secret) && file.contains("잡지DB"), "the settings file holds the connection, not the password");
        // the session password: a password that cannot be checked is not kept; one that was kept is shown only as `unlocked`, found by the
        // drivers before the credential store, forgotten by `dbLockConnection`, by removing the connection and by pointing it at another server
        let again = "Another-비밀-'\"";
        let unlock = unlock_connection(&json!({"cmd": "dbUnlockConnection", "name": "잡지DB", "password": again}), None);
        assert_eq!(unlock["ok"], false, "{unlock}");
        assert!(matches!(unlock["errorKind"].as_str(), Some("network" | "timeout")), "{unlock}");
        assert!(!credentials::session_unlocked("잡지DB"), "a password that could not be checked is not kept");
        assert_eq!(unlock_connection(&json!({"name": "없는DB", "password": "x"}), None)["errorKind"], "notFound");
        assert_eq!(unlock_connection(&json!({"name": "잡지DB", "password": ""}), None)["errorKind"], "invalid");
        credentials::session_set("잡지DB", Secret::new(again));
        assert_eq!(credentials::resolve("잡지DB").unwrap().expose(), again, "the session wins over the saved password");
        let list = connections();
        assert_eq!((&list["connections"][0]["unlocked"], &list["connections"][0]["hasPassword"]), (&json!(true), &json!(true)), "{list}");
        assert!(!list.to_string().contains(again));
        assert_eq!(lock_connection(&json!({"name": "잡지DB"}))["ok"], true);
        assert_eq!(connections()["connections"][0]["unlocked"], false);
        assert_eq!(credentials::resolve("잡지DB").unwrap().expose(), secret, "then the saved one");
        credentials::session_set("잡지DB", Secret::new(again));
        let moved = save_connection(&json!({"cmd": "dbSaveConnection", "name": "잡지DB", "driver": "postgres", "host": "127.0.0.1", "port": 2,
            "database": "magazine", "user": "folio", "savePassword": true, "tls": {"mode": "disable"}}));
        assert_eq!(moved["ok"], true, "{moved}");
        assert!(!credentials::session_unlocked("잡지DB"), "another port is another server: its session password is forgotten");
        // a missing connection
        assert_eq!(tables(&json!({"connection": "없는DB"}), None)["errorKind"], "notFound");
        credentials::session_set("잡지DB", Secret::new(again));
        assert_eq!(remove_connection(&json!({"name": "잡지DB"}))["ok"], true);
        assert!(!credentials::session_unlocked("잡지DB"), "removing a connection forgets its session password");
        assert_eq!(connections()["connections"].as_array().map(Vec::len), Some(0));
        assert_eq!(credentials::load("잡지DB").unwrap(), None, "the saved password went with it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn capabilities_and_the_credential_store_self_test() {
        if !run_with_test_store("capabilities_and_the_credential_store_self_test") { return; }
        let caps = capabilities();
        assert_eq!(caps["platform"], std::env::consts::OS, "{caps}");
        assert_eq!(caps["sessionPasswords"], true, "{caps}");
        assert!(caps["credentialStore"]["available"].is_boolean() && caps["credentialStore"]["kind"].is_string(), "{caps}");
        assert_eq!(caps["tlsVerify"]["available"], true, "{caps}");
        let r = credential_store_self_test();
        assert_eq!(r["store"]["kind"], "memory", "{r}");
        assert_eq!(r["ok"], true, "{r}");
        let steps: Vec<_> = r["steps"].as_array().unwrap().iter().map(|s| s["step"].as_str().unwrap().to_owned()).collect();
        assert_eq!(steps, ["save", "read", "remove", "gone"], "{r}");
        assert!(!r.to_string().contains("folio self test"), "the throwaway value is not returned: {r}");
    }
}

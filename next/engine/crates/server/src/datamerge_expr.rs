//! Data Merge computed fields on the host side: the document keeps the list (`setDataMergeComputedFields`, label `folio.datamerge`); here
//! they become fields of the data source the panel shows and the merge uses, `dataMergeEvaluate` answers what an expression gives for a
//! record, an edit is checked against the data source's columns, and an IDML export gets a CSV with the values (InDesign cannot compute).
//! The language and its rules are `folio_idml::datamerge_expr`. Rules of the design: `artifacts/dbmerge-20261119/DESIGN.ko.md`.
use crate::{Session, edit::EditState};
use folio_edit::{Command, commands::RegisteredCommand};
use folio_idml::datamerge::Source;
use folio_idml::datamerge_expr::{self as expr, Applied, ComputedField};
use serde_json::{Value, json};

/// The computed fields of the open document (its label `folio.datamerge`).
pub(crate) fn document_computed(session: Option<&Session>) -> Vec<ComputedField> {
    session.and_then(|s| s.edit.as_ref())
        .map(|e| e.lock().expect("shared state must remain unpoisoned after each operation").editor.data_merge_computed())
        .unwrap_or_default()
}

/// The computed fields a request means: its own `computed` list (a preview of what is not applied yet), `computed: false` for none,
/// else the document's.
pub(crate) fn computed_of(req: &Value, session: Option<&Session>) -> Result<Vec<ComputedField>, String> {
    match &req["computed"] {
        Value::Array(_) => serde_json::from_value(req["computed"].clone()).map_err(|e| format!("computed is a list of {{name, expr}}: {e}")),
        Value::Bool(false) => Ok(Vec::new()),
        _ => Ok(document_computed(session)),
    }
}

/// What the panel and the merge need to know about the computed fields of a source.
fn computed_json(applied: &Applied, computed: &[ComputedField]) -> Value {
    json!({
        "fields": computed, "added": applied.added,
        "definitionErrors": applied.definition_errors, "recordErrors": applied.record_errors, "errorCount": applied.error_count,
    })
}

fn no_computed() -> Value { json!({"fields": [], "added": [], "definitionErrors": [], "recordErrors": [], "errorCount": 0}) }

/// The source with the computed fields (the request's or the document's) added, as JSON, and the `computed` part of the reply.
pub(crate) fn attach(source: Source, req: &Value, session: Option<&Session>) -> Result<(Value, Value), String> {
    let computed = computed_of(req, session)?;
    if computed.is_empty() { return Ok((json!(source), no_computed())); }
    let applied = expr::apply(&source, &computed);
    let reply = computed_json(&applied, &computed);
    Ok((json!(applied.source), reply))
}

/// The data source the document remembers, as the file has it (the computed fields are not in it), from the reply of
/// `datamerge_link::linked`.
fn raw_of(reply: &Value) -> Result<Option<Source>, String> {
    match reply.get("source").filter(|s| s.is_object()) {
        Some(s) => serde_json::from_value(s.clone()).map(Some).map_err(|e| e.to_string()),
        None => Ok(None),
    }
}

/// `dataMergeSource` without a path: the data source the document remembers, with the document's computed fields as fields of it
/// (`computed.added` says which); `computed.fields` is the definitions, also when the source cannot be read.
pub(crate) fn linked(req: &Value, session: Option<&Session>) -> Result<Value, String> {
    let mut reply = crate::datamerge_link::linked(session)?;
    match raw_of(&reply)? {
        Some(source) => {
            let (shown, computed) = attach(source, req, session)?;
            reply["source"] = shown;
            reply["computed"] = computed;
        }
        None => {
            let defined = computed_of(req, session)?;
            reply["computed"] = json!({"fields": defined, "added": [], "definitionErrors": [], "recordErrors": [], "errorCount": 0});
        }
    }
    Ok(reply)
}

// ---------------------------------------------------------------------------------------------------------------------
// dataMergeEvaluate

fn record_source(values: &serde_json::Map<String, Value>) -> Result<Source, String> {
    let mut fields = Vec::new();
    let mut record = Vec::new();
    for (name, value) in values {
        fields.push(expr::field_of_name(name).map_err(|e| e.message)?);
        record.push(match value { Value::String(s) => s.clone(), Value::Null => String::new(), other => other.to_string() });
    }
    if fields.is_empty() { return Err("record has no fields".into()); }
    Ok(Source {
        encoding: "ASCII".into(), delimiter: ",".into(), fields, records: vec![record],
        options: folio_idml::datamerge::Options { delimiter: folio_idml::datamerge::Delimiter::Comma, encoding: folio_idml::datamerge::Encoding::Ascii, platform: folio_idml::datamerge::Platform::Pc, preserve_spaces: false },
    })
}

/// `dataMergeEvaluate {expr? | fields?, computed?, record?, path?, describe?}`: what a computed-field expression gives.
/// - the data source is: the `record` as `{field: value}` when it is an object, else the project file `path` (with the import options
///   of `dataMergeSource`), else the data source the document remembers; none is fine for the syntax alone;
/// - `record` (a 1-based number, default 1) is the record the values are for;
/// - with `expr`: its value for the record (`value`, `kind` text / number / date / bool / empty) or its error (`error`: code, pos, end,
///   args, message; positions are characters from 0), next to the computed fields `computed` (default: the document's) that it may use;
/// - with `fields` (a list of `{name, expr}`) or with nothing (the document's): each field's value and error for the record, and the
///   errors of every record (`recordErrors`, the first 500);
/// - `describe: true` adds the function list.
pub(crate) fn evaluate(req: &Value, session: Option<&Session>) -> Result<Value, String> {
    let mut reply = json!({"type": "dataMergeEvaluate"});
    if req["describe"].as_bool() == Some(true) {
        reply["functions"] = json!(expr::functions());
        reply["limits"] = json!({"maxExprChars": expr::MAX_EXPR_CHARS, "maxComputedFields": expr::MAX_COMPUTED_FIELDS, "maxRecordErrors": expr::MAX_RECORD_ERRORS});
    }
    let source: Option<Source> = if let Some(values) = req["record"].as_object() { Some(record_source(values)?) }
        else if let Some(relative) = req["path"].as_str() {
            let project = &session.and_then(|s| s.folio.as_ref()).ok_or("Open a project document")?.opened.project;
            Some(crate::datamerge::parse_request(&crate::datamerge::read(project, relative)?, req)?)
        } else if session.is_some_and(|s| s.folio.is_some()) { raw_of(&crate::datamerge_link::linked(session)?)? } else { None };
    let single = req["expr"].as_str();
    let fields_given = req["fields"].is_array();
    if single.is_none() && !fields_given && req["describe"].as_bool() == Some(true) { return Ok(reply); }
    let count = source.as_ref().map_or(0, |s| s.records.len());
    let record = match &req["record"] { Value::Number(n) => n.as_u64().unwrap_or(0) as usize, _ => 1 };
    if source.is_some() && (record < 1 || record > count) { return Err(format!("record {record} is out of range (1 to {count})")); }
    reply["record"] = json!(record);
    reply["recordCount"] = json!(count);
    if let Some(text) = single {
        // the fields the expression may use: the source's and the computed ones above (the request's or the document's)
        let defined = computed_of(req, session)?;
        let Some(source) = source else {
            reply["error"] = match expr::compile(text, None, &[]) { Ok(_) => Value::Null, Err(e) => json!(e) };
            reply["syntaxOnly"] = json!(true);
            return Ok(reply);
        };
        let applied = expr::apply(&source, &defined);
        let outcome = expr::compile(text, Some(&applied.source.fields), &[]).and_then(|c| expr::evaluate(&c, &applied.source.records[record - 1]));
        match outcome {
            Ok(v) => { reply["value"] = json!(v.text); reply["kind"] = json!(v.kind); reply["error"] = Value::Null; }
            Err(e) => { reply["error"] = json!(e); }
        }
        return Ok(reply);
    }
    let fields: Vec<ComputedField> = if fields_given {
        serde_json::from_value(req["fields"].clone()).map_err(|e| format!("fields is a list of {{name, expr}}: {e}"))?
    } else { document_computed(session) };
    let Some(source) = source else {
        // no data source: the syntax of each expression, and the names
        let mut definition_errors = Vec::new();
        for cf in &fields {
            let error = expr::field_of_name(&cf.name).err().or_else(|| expr::compile(&cf.expr, None, &[]).err());
            if let Some(error) = error { definition_errors.push(expr::DefinitionError { field: cf.name.clone(), error }); }
        }
        reply["fields"] = json!(fields.iter().map(|f| json!({"name": f.name, "expr": f.expr})).collect::<Vec<_>>());
        reply["definitionErrors"] = json!(definition_errors);
        reply["syntaxOnly"] = json!(true);
        return Ok(reply);
    };
    let applied = expr::apply(&source, &fields);
    let rows: Vec<Value> = fields.iter().enumerate().map(|(i, f)| {
        let mut row = json!({"name": f.name, "expr": f.expr});
        if let Some(a) = applied.added.iter().find(|a| a.computed == i) {
            row["value"] = json!(applied.source.records[record - 1][a.column]);
            if let Some(e) = applied.record_errors.iter().find(|e| e.record == record && e.field == f.name) { row["error"] = json!(e.error); }
        }
        if let Some(e) = applied.definition_errors.iter().find(|e| e.field == f.name) { row["definitionError"] = json!(e.error); }
        row
    }).collect();
    reply["fields"] = json!(rows);
    reply["definitionErrors"] = json!(applied.definition_errors);
    reply["recordErrors"] = json!(applied.record_errors);
    reply["errorCount"] = json!(applied.error_count);
    Ok(reply)
}

// ---------------------------------------------------------------------------------------------------------------------
// an edit with `setDataMergeComputedFields`

/// Before the edit: the expressions are checked against the columns of the data source the document remembers (a column that is not
/// there is an error; the edit itself only checks the syntax and the names). The refusal lists every error (`errors`).
pub(crate) fn prepare(commands: &[Command], cur: &Session, es: &EditState) -> Result<(), Value> {
    for command in commands {
        let Command::Registered(RegisteredCommand::SetDataMergeComputedFields { fields }) = command else { continue };
        let Ok(reply) = crate::datamerge_link::linked_in(cur, es) else { continue };
        let Ok(Some(source)) = raw_of(&reply) else { continue };
        let applied = expr::apply(&source, fields);
        if let Some(first) = applied.definition_errors.first() {
            return Err(json!({
                "ok": false, "code": "expr",
                "error": format!("{}: {} (character {})", first.field, first.error.message, first.error.pos + 1),
                "errors": applied.definition_errors,
            }));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// the IDML export

/// An IDML export of a document with computed fields: InDesign cannot compute them, so the data source InDesign gets is a CSV with the
/// source's columns and the values of the computed ones, in the project (`assets/<document>/datamerge/<hash>.csv`; the name is the
/// hash of the content, so exporting again writes nothing new). The exported `Resources/Preferences.xml` names that file (this
/// computer's path, as every path an export writes), and the relative picture paths of image fields become full paths (InDesign reads
/// them from the folder of the CSV, which is not the folder of the data source). Nothing is stored in the document.
/// Returns what the export report says (`None`: the document has none, or its source cannot be read).
pub(crate) fn export_prepare(cur: &Session, es: &EditState, changes: &mut std::collections::BTreeMap<String, Option<String>>) -> Result<Option<Value>, String> {
    let computed = es.editor.data_merge_computed();
    let Some(folio) = cur.folio.as_ref() else { return Ok(None) };
    if computed.is_empty() || es.editor.data_source().is_none() { return Ok(None); }
    let Ok(reply) = crate::datamerge_link::linked_in(cur, es) else { return Ok(None) };
    let Ok(Some(raw)) = raw_of(&reply) else { return Ok(Some(json!({"written": false, "count": computed.len(), "why": "the data source cannot be read"}))) };
    let applied = expr::apply(&raw, &computed);
    let root = &folio.opened.project.root;
    // relative pictures are relative to the folder of the data source in the project
    let folder = reply["path"].as_str().and_then(|p| std::path::Path::new(p).parent().map(|d| root.join(d))).unwrap_or_else(|| root.clone());
    let full = |value: &str| -> String {
        if std::path::Path::new(value).is_absolute() { return value.to_owned(); }
        let mut path = folder.clone();
        for part in value.split(['/', '\\']).filter(|p| !p.is_empty() && *p != ".") { path.push(part); }
        path.display().to_string()
    };
    let (bytes, file_type) = expr::export_csv(&applied.source, &full);
    let stem = folio_project::paths::fix_name(folio.opened.file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into()).as_str());
    let directory = root.join("assets").join(&stem).join("datamerge");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let name = format!("{}.csv", &folio_project::hash::sha256_bytes(&bytes)[..16]);
    let target = directory.join(&name);
    if !target.is_file() { folio_project::atomic::write_bytes(&target, &bytes).map_err(|e| e.to_string())?; }
    // the preferences with the CSV as the data source (the document's own stay as they are)
    let mut copy = folio_edit::Editor::new(es.editor.parts().clone());
    let select: Command = RegisteredCommand::SelectDataSource {
        path: String::new(), source: Some(target.display().to_string()), file_type: Some(file_type.unwrap_or("").to_owned()),
        delimiter: None, encoding: None, platform: None, preserve_spaces: None,
    }.into();
    copy.apply("Export data source", &[select]).map_err(|e| e.message)?;
    const PREFERENCES: &str = "Resources/Preferences.xml";
    changes.insert(PREFERENCES.into(), copy.part(PREFERENCES).map(str::to_owned));
    Ok(Some(json!({
        "written": true, "count": applied.added.len(), "relative": format!("assets/{stem}/datamerge/{name}"),
        "records": applied.source.records.len(), "errorCount": applied.error_count, "encoding": if file_type.is_some() { "ASCII" } else { "UTF-16LE" },
    })))
}

//! A query definition → one SELECT for a database kind, with bound values.
//!
//! Identifiers (table, columns) must be ones the database listed (`dbTables`, `dbColumns`) and are quoted for the kind; every
//! value is a bound parameter (bound as text: the database reads it in the type of the column it is compared with), so
//! nothing a user types becomes SQL.
use crate::error::{DbError, Result};
use crate::model::{ColumnInfo, Driver, Query, TableRef};
use crate::params::{Date, Month, Resolved};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Built {
    pub sql: String,
    pub binds: Vec<String>,
    /// `SELECT COUNT(*)` of the same rows (no limit), with the same binds.
    pub count_sql: String,
}

/// Quoting per kind: PostgreSQL and SQLite `"…"`, MySQL `` `…` ``, SQL Server `[…]` (a `]` inside is doubled); the quote inside is doubled.
pub fn quote(driver: Driver, name: &str) -> String {
    match driver {
        Driver::Mysql => format!("`{}`", name.replace('`', "``")),
        Driver::Sqlserver => format!("[{}]", name.replace(']', "]]")),
        Driver::Postgres | Driver::Sqlite => format!("\"{}\"", name.replace('"', "\"\"")),
    }
}

/// The placeholder of bound value `n` (1-based).
pub fn placeholder(driver: Driver, n: usize) -> String {
    match driver { Driver::Postgres => format!("${n}"), Driver::Sqlserver => format!("@P{n}"), Driver::Mysql | Driver::Sqlite => "?".into() }
}

fn as_text(driver: Driver, column: &str) -> String {
    match driver {
        Driver::Mysql => format!("CAST({column} AS CHAR)"),
        Driver::Sqlserver => format!("CAST({column} AS NVARCHAR(MAX))"),
        Driver::Postgres | Driver::Sqlite => format!("CAST({column} AS TEXT)"),
    }
}

pub fn table_name(driver: Driver, table: &TableRef) -> String {
    match table.schema.as_deref().filter(|s| !s.is_empty() && driver != Driver::Sqlite) {
        Some(schema) => format!("{}.{}", quote(driver, schema), quote(driver, &table.name)),
        None => quote(driver, &table.name),
    }
}

struct Ctx<'a> {
    driver: Driver,
    columns: &'a [ColumnInfo],
    params: &'a BTreeMap<String, Resolved>,
    binds: Vec<String>,
}

impl Ctx<'_> {
    fn column(&self, name: &str) -> Result<String> {
        // exactly a listed column (as the database spells it): nothing else reaches the SQL
        self.columns.iter().find(|c| c.name == name).map(|c| quote(self.driver, &c.name))
            .ok_or_else(|| DbError::new(crate::Kind::NotFound, format!("the table has no column {name:?}")))
    }
    fn bind(&mut self, value: String) -> String {
        self.binds.push(value);
        placeholder(self.driver, self.binds.len())
    }
    /// A value of a condition: a literal or `{param: name}`.
    fn value(&self, v: &Value) -> Result<Resolved> {
        if let Some(name) = v.get("param").and_then(Value::as_str) {
            return self.params.get(name).cloned().ok_or_else(|| DbError::invalid(format!("no parameter is called {name}")));
        }
        Ok(match v {
            Value::String(s) => Resolved::Text(s.clone()),
            Value::Number(n) => Resolved::Number(n.to_string()),
            Value::Bool(b) => Resolved::Text(b.to_string()),
            Value::Array(items) => Resolved::List(items.iter().map(|i| match self.value(i)? {
                Resolved::Text(s) | Resolved::Number(s) => Ok(s),
                Resolved::Date(d) => Ok(d.to_string()),
                Resolved::Month(m) => Ok(m.to_string()),
                Resolved::List(_) => Err(DbError::invalid("a list inside a list")),
            }).collect::<Result<_>>()?),
            _ => return Err(DbError::invalid("a condition value is a text, a number, a list or {param}")),
        })
    }
    fn scalar(&self, v: Option<&Value>, op: &str) -> Result<String> {
        match self.value(v.ok_or_else(|| DbError::invalid(format!("{op} needs a value")))?)? {
            Resolved::Text(s) | Resolved::Number(s) => Ok(s),
            Resolved::Date(d) => Ok(d.to_string()),
            Resolved::Month(m) => Ok(m.first().to_string()),
            Resolved::List(_) => Err(DbError::invalid(format!("{op} needs one value, not a list"))),
        }
    }
    fn pair(&self, v: Option<&Value>, op: &str) -> Result<(String, String)> {
        match self.value(v.ok_or_else(|| DbError::invalid(format!("{op} needs two values")))?)? {
            Resolved::List(l) if l.len() == 2 => Ok((l[0].clone(), l[1].clone())),
            _ => Err(DbError::invalid(format!("{op} needs a list of two values"))),
        }
    }
    fn like(&mut self, col: &str, text: &str, before: &str, after: &str) -> String {
        // `!` escapes the wildcards: the one escape character every kind reads the same (MySQL reads `\` inside literals)
        let escaped: String = text.chars().flat_map(|c| if matches!(c, '!' | '%' | '_' | '[') { vec!['!', c] } else { vec![c] }).collect();
        let op = if self.driver == Driver::Postgres { "ILIKE" } else { "LIKE" };
        let p = self.bind(format!("{before}{escaped}{after}"));
        format!("{} {op} {p} ESCAPE '!'", as_text(self.driver, col))
    }
    fn condition(&mut self, c: &Value) -> Result<String> {
        let column = c.get("column").and_then(Value::as_str).ok_or_else(|| DbError::invalid("a condition needs a column"))?;
        let op = c.get("op").and_then(Value::as_str).ok_or_else(|| DbError::invalid("a condition needs an operator"))?;
        let col = self.column(column)?;
        let v = c.get("value");
        Ok(match op {
            "=" | "!=" | "<" | "<=" | ">" | ">=" => {
                let value = self.scalar(v, op)?;
                let sql_op = if op == "!=" { "<>" } else { op };
                format!("{col} {sql_op} {}", self.bind(value))
            }
            "contains" => { let t = self.scalar(v, op)?; self.like(&col, &t, "%", "%") }
            "startsWith" => { let t = self.scalar(v, op)?; self.like(&col, &t, "", "%") }
            "endsWith" => { let t = self.scalar(v, op)?; self.like(&col, &t, "%", "") }
            "in" => {
                let list = match self.value(v.ok_or_else(|| DbError::invalid("in needs a list"))?)? {
                    Resolved::List(l) => l,
                    Resolved::Text(s) | Resolved::Number(s) => vec![s],
                    _ => return Err(DbError::invalid("in needs a list")),
                };
                if list.is_empty() { return Ok("1 = 0".into()); }
                if list.len() > 1000 { return Err(DbError::invalid("in takes at most 1000 values")); }
                let ps: Vec<String> = list.into_iter().map(|s| self.bind(s)).collect();
                format!("{col} IN ({})", ps.join(", "))
            }
            "between" => {
                let (a, b) = self.pair(v, op)?;
                format!("{col} BETWEEN {} AND {}", self.bind(a), self.bind(b))
            }
            "isEmpty" => format!("({col} IS NULL OR {} = '')", as_text(self.driver, &col)),
            "isNotEmpty" => format!("({col} IS NOT NULL AND {} <> '')", as_text(self.driver, &col)),
            "inMonth" => {
                let month = match self.value(v.ok_or_else(|| DbError::invalid("inMonth needs a month"))?)? {
                    Resolved::Month(m) => m,
                    Resolved::Date(d) => Month::of(d),
                    Resolved::Text(s) => Month::parse(&s).or_else(|| Date::parse(&s).map(Month::of)).ok_or_else(|| DbError::invalid("inMonth needs a month (YYYY-MM)"))?,
                    _ => return Err(DbError::invalid("inMonth needs a month (YYYY-MM)")),
                };
                // the first day of the month up to (not including) the first of the next: a date or a time of day
                let (from, to) = (month.first(), month.first().add_months(1));
                format!("({col} >= {} AND {col} < {})", self.bind(from.to_string()), self.bind(to.to_string()))
            }
            "inDateRange" => {
                let (a, b) = self.pair(v, op)?;
                let from = Date::parse(&a).ok_or_else(|| DbError::invalid("inDateRange needs two dates (YYYY-MM-DD)"))?;
                let to = Date::parse(&b).ok_or_else(|| DbError::invalid("inDateRange needs two dates (YYYY-MM-DD)"))?;
                // both days included
                format!("({col} >= {} AND {col} < {})", self.bind(from.to_string()), self.bind(to.next_day().to_string()))
            }
            other => return Err(DbError::invalid(format!("unknown operator {other}"))),
        })
    }
    /// `{all:[…]}` / `{any:[…]}` nest two levels at most (design 2).
    fn filter(&mut self, f: &Value, depth: usize) -> Result<String> {
        let group = |key: &str| f.get(key).and_then(Value::as_array);
        let (items, joiner) = match (group("all"), group("any")) {
            (Some(items), None) => (items, " AND "),
            (None, Some(items)) => (items, " OR "),
            (None, None) => return self.condition(f),
            _ => return Err(DbError::invalid("a group is all or any, not both")),
        };
        if depth >= 2 { return Err(DbError::invalid("condition groups nest two levels at most")); }
        if items.is_empty() { return Ok(if joiner == " AND " { "1 = 1".into() } else { "1 = 0".into() }); }
        let parts = items.iter().map(|i| self.filter(i, depth + 1)).collect::<Result<Vec<_>>>()?;
        Ok(format!("({})", parts.join(joiner)))
    }
}

/// The SELECT of a query definition over `table`, whose columns the database listed as `columns`. `cap`: at most this many
/// rows come back (the query's own limit when it is smaller).
pub fn build(query: &Query, driver: Driver, table: &TableRef, columns: &[ColumnInfo], params: &BTreeMap<String, Resolved>, cap: usize) -> Result<Built> {
    let mut ctx = Ctx { driver, columns, params, binds: Vec::new() };
    let select = if query.columns.is_empty() {
        if columns.is_empty() { return Err(DbError::invalid("the table has no columns")); }
        columns.iter().map(|c| quote(driver, &c.name)).collect::<Vec<_>>()
    } else {
        query.columns.iter().map(|c| ctx.column(c)).collect::<Result<Vec<_>>>()?
    };
    let from = table_name(driver, table);
    let filter = match &query.filter { Some(f) if !f.is_null() => format!(" WHERE {}", ctx.filter(f, 0)?), _ => String::new() };
    let count_sql = format!("SELECT COUNT(*) FROM {from}{filter}");
    let mut order = Vec::new();
    for o in &query.order_by {
        let dir = match o.dir.as_str() { "asc" => "ASC", "desc" => "DESC", d => return Err(DbError::invalid(format!("order is asc or desc, not {d}"))) };
        order.push(format!("{} {dir}", ctx.column(&o.column)?));
    }
    let order = if order.is_empty() { String::new() } else { format!(" ORDER BY {}", order.join(", ")) };
    let limit = query.limit.map_or(cap as u64, |l| l.min(cap as u64));
    let offset = query.offset.unwrap_or(0);
    if driver == Driver::Sqlserver {
        // SQL Server has no LIMIT: OFFSET … FETCH, which needs an ORDER BY (none asked: `(SELECT NULL)`, the server's own order);
        // FETCH NEXT 0 is refused by the server, TOP (0) is the empty answer
        if limit == 0 { return Ok(Built { sql: format!("SELECT TOP (0) {} FROM {from}{filter}", select.join(", ")), binds: ctx.binds, count_sql }); }
        let order = if order.is_empty() { " ORDER BY (SELECT NULL)".to_owned() } else { order };
        return Ok(Built { sql: format!("SELECT {} FROM {from}{filter}{order} OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY", select.join(", ")), binds: ctx.binds, count_sql });
    }
    let tail = match (driver, offset) {
        (_, 0) => format!(" LIMIT {limit}"),
        _ => format!(" LIMIT {limit} OFFSET {offset}"),
    };
    Ok(Built { sql: format!("SELECT {} FROM {from}{filter}{order}{tail}", select.join(", ")), binds: ctx.binds, count_sql })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Param, ParamType};
    use serde_json::json;

    fn cols() -> Vec<ColumnInfo> {
        ["id", "title", "price", "issue_date", "status", "page", "제목 \"따옴\""].iter()
            .map(|n| ColumnInfo { name: (*n).into(), kind: "text".into(), nullable: true }).collect()
    }

    fn query() -> Query {
        serde_json::from_value(json!({
            "connection": "잡지DB",
            "table": {"schema": "public", "name": "ads"},
            "columns": ["id", "title", "price", "issue_date"],
            "filter": {"all": [{"column": "issue_date", "op": "inMonth", "value": {"param": "월"}},
                               {"any": [{"column": "status", "op": "=", "value": "게재"}, {"column": "status", "op": "=", "value": "확정"}]}]},
            "orderBy": [{"column": "page", "dir": "asc"}, {"column": "id", "dir": "desc"}],
            "limit": 500,
            "params": [{"name": "월", "type": "month", "default": "thisMonth"}]
        })).unwrap()
    }

    fn resolved(q: &Query) -> BTreeMap<String, Resolved> {
        crate::params::resolve(&q.params, &Default::default(), Date { y: 2026, m: 11, d: 19 }).unwrap()
    }

    #[test]
    fn golden_sql_per_kind() {
        let q = query();
        let t = q.table.clone().unwrap();
        let golden = [
            (Driver::Postgres, r#"SELECT "id", "title", "price", "issue_date" FROM "public"."ads" WHERE (("issue_date" >= $1 AND "issue_date" < $2) AND ("status" = $3 OR "status" = $4)) ORDER BY "page" ASC, "id" DESC LIMIT 500"#),
            (Driver::Mysql, "SELECT `id`, `title`, `price`, `issue_date` FROM `public`.`ads` WHERE ((`issue_date` >= ? AND `issue_date` < ?) AND (`status` = ? OR `status` = ?)) ORDER BY `page` ASC, `id` DESC LIMIT 500"),
            (Driver::Sqlite, r#"SELECT "id", "title", "price", "issue_date" FROM "ads" WHERE (("issue_date" >= ? AND "issue_date" < ?) AND ("status" = ? OR "status" = ?)) ORDER BY "page" ASC, "id" DESC LIMIT 500"#),
            (Driver::Sqlserver, "SELECT [id], [title], [price], [issue_date] FROM [public].[ads] WHERE (([issue_date] >= @P1 AND [issue_date] < @P2) AND ([status] = @P3 OR [status] = @P4)) ORDER BY [page] ASC, [id] DESC OFFSET 0 ROWS FETCH NEXT 500 ROWS ONLY"),
        ];
        for (driver, sql) in golden {
            let b = build(&q, driver, &t, &cols(), &resolved(&q), 50_000).unwrap();
            assert_eq!(b.sql, sql, "{driver:?}");
            assert_eq!(b.binds, ["2026-11-01", "2026-12-01", "게재", "확정"], "{driver:?}");
        }
        let b = build(&q, Driver::Postgres, &t, &cols(), &resolved(&q), 50).unwrap();
        assert!(b.sql.ends_with("LIMIT 50"), "the cap wins over a larger limit");
        assert_eq!(b.count_sql, r#"SELECT COUNT(*) FROM "public"."ads" WHERE (("issue_date" >= $1 AND "issue_date" < $2) AND ("status" = $3 OR "status" = $4))"#);
    }

    #[test]
    fn every_operator() {
        let mut q = query();
        q.order_by.clear();
        q.limit = None;
        q.offset = Some(10);
        q.columns = vec!["제목 \"따옴\"".into()];
        q.params = vec![Param { name: "목록".into(), kind: ParamType::List, default: Some(json!(["a", "b"])) }];
        q.filter = Some(json!({"all": [
            {"column": "title", "op": "contains", "value": "50%_off!"},
            {"column": "title", "op": "startsWith", "value": "새"},
            {"column": "title", "op": "endsWith", "value": "호"},
            {"column": "price", "op": "between", "value": [1000, 2000]},
            {"column": "status", "op": "in", "value": {"param": "목록"}},
            {"column": "status", "op": "in", "value": []},
            {"column": "title", "op": "isEmpty"},
            {"column": "title", "op": "isNotEmpty"},
            {"column": "issue_date", "op": "inDateRange", "value": ["2026-02-01", "2026-02-28"]},
            {"column": "price", "op": "!=", "value": 3}]}));
        let t = TableRef { schema: None, name: "광고".into() };
        let b = build(&q, Driver::Sqlite, &t, &cols(), &crate::params::resolve(&q.params, &Default::default(), Date { y: 2026, m: 1, d: 1 }).unwrap(), 50_000).unwrap();
        assert_eq!(b.sql, concat!(r#"SELECT "제목 ""따옴""" FROM "광고" WHERE (CAST("title" AS TEXT) LIKE ? ESCAPE '!' AND CAST("title" AS TEXT) LIKE ? ESCAPE '!' AND "#,
            r#"CAST("title" AS TEXT) LIKE ? ESCAPE '!' AND "price" BETWEEN ? AND ? AND "status" IN (?, ?) AND 1 = 0 AND ("title" IS NULL OR CAST("title" AS TEXT) = '') AND "#,
            r#"("title" IS NOT NULL AND CAST("title" AS TEXT) <> '') AND ("issue_date" >= ? AND "issue_date" < ?) AND "price" <> ?) LIMIT 50000 OFFSET 10"#));
        assert_eq!(b.binds, ["%50!%!_off!!%", "새%", "%호", "1000", "2000", "a", "b", "2026-02-01", "2026-03-01", "3"]);
        let b = build(&q, Driver::Postgres, &t, &cols(), &crate::params::resolve(&q.params, &Default::default(), Date { y: 2026, m: 1, d: 1 }).unwrap(), 50_000).unwrap();
        assert!(b.sql.contains(r#"CAST("title" AS TEXT) ILIKE $1 ESCAPE '!'"#), "{}", b.sql);
        let b = build(&q, Driver::Sqlserver, &t, &cols(), &crate::params::resolve(&q.params, &Default::default(), Date { y: 2026, m: 1, d: 1 }).unwrap(), 50_000).unwrap();
        assert_eq!(b.sql, concat!(r#"SELECT [제목 "따옴"] FROM [광고] WHERE (CAST([title] AS NVARCHAR(MAX)) LIKE @P1 ESCAPE '!' AND CAST([title] AS NVARCHAR(MAX)) LIKE @P2 ESCAPE '!' AND "#,
            r#"CAST([title] AS NVARCHAR(MAX)) LIKE @P3 ESCAPE '!' AND [price] BETWEEN @P4 AND @P5 AND [status] IN (@P6, @P7) AND 1 = 0 AND ([title] IS NULL OR CAST([title] AS NVARCHAR(MAX)) = '') AND "#,
            r#"([title] IS NOT NULL AND CAST([title] AS NVARCHAR(MAX)) <> '') AND ([issue_date] >= @P8 AND [issue_date] < @P9) AND [price] <> @P10) ORDER BY (SELECT NULL) OFFSET 10 ROWS FETCH NEXT 50000 ROWS ONLY"#));
        assert_eq!(b.binds, ["%50!%!_off!!%", "새%", "%호", "1000", "2000", "a", "b", "2026-02-01", "2026-03-01", "3"], "`[` `%` `_` are escaped for LIKE (SQL Server reads [ as a set)");
        q.limit = Some(0);
        let b = build(&q, Driver::Sqlserver, &t, &cols(), &crate::params::resolve(&q.params, &Default::default(), Date { y: 2026, m: 1, d: 1 }).unwrap(), 50_000).unwrap();
        assert!(b.sql.starts_with("SELECT TOP (0) [제목 \"따옴\"] FROM [광고] WHERE ") && !b.sql.contains("FETCH"), "{}", b.sql);
    }

    #[test]
    fn injection_attempts_are_refused_or_bound() {
        let mut q = query();
        let t = q.table.clone().unwrap();
        // a column that is not listed never reaches the SQL
        for bad in ["id; DROP TABLE ads", "id\" OR \"1\"=\"1", "ID", "(SELECT 1)"] {
            q.columns = vec![bad.into()];
            assert_eq!(build(&q, Driver::Postgres, &t, &cols(), &resolved(&q), 10).unwrap_err().kind, crate::Kind::NotFound, "{bad}");
        }
        q.columns = vec![];
        q.order_by = vec![crate::model::Order { column: "id".into(), dir: "desc; DROP TABLE ads".into() }];
        assert_eq!(build(&q, Driver::Postgres, &t, &cols(), &resolved(&q), 10).unwrap_err().kind, crate::Kind::Invalid);
        q.order_by.clear();
        // a value is a bound parameter however it looks
        q.filter = Some(json!({"column": "title", "op": "=", "value": "'; DROP TABLE ads; --"}));
        let b = build(&q, Driver::Mysql, &t, &cols(), &resolved(&q), 10).unwrap();
        assert!(!b.sql.contains("DROP"), "{}", b.sql);
        assert_eq!(b.binds, ["'; DROP TABLE ads; --"]);
        q.filter = Some(json!({"column": "title", "op": "= 1 OR 1=1 --", "value": "x"}));
        assert!(build(&q, Driver::Mysql, &t, &cols(), &resolved(&q), 10).is_err());
        // nesting deeper than two levels
        q.filter = Some(json!({"all": [{"any": [{"all": [{"column": "id", "op": "=", "value": 1}]}]}]}));
        assert!(build(&q, Driver::Sqlite, &t, &cols(), &resolved(&q), 10).is_err());
        assert_eq!(quote(Driver::Mysql, "a`b"), "`a``b`");
        assert_eq!(quote(Driver::Sqlserver, "a]b"), "[a]]b]");
        // SQL Server: a listed name with `]` is doubled inside [..]; anything not listed never reaches the SQL
        let mut c = cols();
        c.push(ColumnInfo { name: "x]; DROP TABLE ads --".into(), kind: "nvarchar".into(), nullable: true });
        q.filter = None;
        q.columns = vec!["x]; DROP TABLE ads --".into()];
        let b = build(&q, Driver::Sqlserver, &t, &c, &resolved(&q), 10).unwrap();
        assert!(b.sql.starts_with("SELECT [x]]; DROP TABLE ads --] FROM [public].[ads] ORDER BY (SELECT NULL) OFFSET 0 ROWS FETCH NEXT 10 ROWS ONLY"), "{}", b.sql);
        for bad in ["id]; DROP TABLE ads --", "[id]", "id] OR 1=1 --"] {
            q.columns = vec![bad.into()];
            assert_eq!(build(&q, Driver::Sqlserver, &t, &cols(), &resolved(&q), 10).unwrap_err().kind, crate::Kind::NotFound, "{bad}");
        }
    }
}

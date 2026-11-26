//! 상호운용 표의 읽기 경계. 가져오기·내보내기 보고서는 이 표를 조회한다.
//! 상태는 필드 전체를 보증하는 주장이다. 미확인은 정확으로 승격하지 않는다.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mapping { Exact, Approximate, FolioOnly, Unclassified }

#[derive(Clone, Debug)]
pub struct Entry<'a> {
    pub item: &'a str,
    pub mapping: Mapping,
    pub appearance: &'a str,
    pub evidence: &'a str,
}

const TABLE: &str = include_str!("../../../../oracle/rules/interop.tsv");

/// 모든 행을 검증해서 반환한다. 잘못된 행을 조용히 버리지 않는다.
pub fn entries() -> Result<Vec<Entry<'static>>, String> { parse(TABLE) }

pub fn lookup(item: &str) -> Result<Option<Entry<'static>>, String> {
    Ok(entries()?.into_iter().find(|e| e.item == item))
}

/// 가져오기 보고서·내보내기 창에서 차이와 미확인 항목을 설명하는 자리.
/// 문서가 실제로 쓰는 항목 목록은 호출자가 제공한다.
pub fn report<'a>(items: impl IntoIterator<Item = &'a str>) -> Result<Vec<Entry<'static>>, String> {
    let catalog = entries()?;
    let mut result = Vec::new();
    for item in items {
        let entry = catalog.iter().find(|e| e.item == item).ok_or_else(|| format!("등록되지 않은 모델 항목: {item}"))?;
        if entry.mapping != Mapping::Exact && !result.iter().any(|e: &Entry<'_>| e.item == item) {
            result.push(entry.clone());
        }
    }
    Ok(result)
}

pub fn parse(table: &str) -> Result<Vec<Entry<'_>>, String> {
    let mut result = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (index, line) in table.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') { continue; }
        let cols: Vec<_> = line.split('\t').collect();
        if cols.len() != 4 || cols[0].is_empty() || cols[2].is_empty() || !seen.insert(cols[0]) {
            return Err(format!("상호운용 표 {}행: 열·설명·고유 항목 확인 필요", index + 1));
        }
        let mapping = match cols[1] {
            "exact" => Mapping::Exact, "approximate" => Mapping::Approximate,
            "folio-only" => Mapping::FolioOnly, "unclassified" => Mapping::Unclassified,
            other => return Err(format!("알 수 없는 대응: {other}")),
        };
        if mapping != Mapping::Unclassified && (cols[3].is_empty() || cols[3] == "-") {
            return Err(format!("근거 없는 분류: {}", cols[0]));
        }
        result.push(Entry { item: cols[0], mapping, appearance: cols[2], evidence: cols[3] });
    }
    Ok(result)
}

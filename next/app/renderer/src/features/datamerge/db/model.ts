// The pure parts of the database source screens: what kind a column is (to offer the operators that fit it), the condition tree the filter
// builder edits, the parameters, and the sentence for each kind of error. No SQL is made here (the engine makes it from the query definition).
import { tr } from '../../../i18n';
import type { ColumnInfo, Condition, Connection, ConnectionSettings, Driver, FilterNode, Group, Hint, Param, ParamType, Query, Refresh } from './api';

export const DRIVERS: Driver[] = ['postgres', 'mysql', 'sqlserver', 'sqlite'];
export const driverName = (d: Driver): string => d === 'postgres' ? 'PostgreSQL' : d === 'mysql' ? 'MySQL / MariaDB' : d === 'sqlite' ? 'SQLite' : 'SQL Server';
export const defaultPort = (d: Driver): number | undefined => d === 'postgres' ? 5432 : d === 'mysql' ? 3306 : d === 'sqlserver' ? 1433 : undefined;

export function emptyConnection(driver: Driver = 'postgres'): ConnectionSettings {
  return { name: '', driver, host: driver === 'sqlite' ? '' : '', port: defaultPort(driver), database: '', user: '', tls: { mode: 'verify' }, password: '', savePassword: true };
}
/** The form for a connection the document points at that this computer does not have: what the hint knows, the rest to fill in. */
export function fromHint(name: string, hint?: Hint): ConnectionSettings {
  const base = emptyConnection(hint?.driver ?? 'postgres');
  return { ...base, name, host: hint?.host ?? '', port: hint?.port ?? (hint?.instance ? undefined : defaultPort(base.driver)), database: hint?.database ?? '', ...(hint?.instance ? { instance: hint.instance } : {}) };
}
export const toSettings = (c: Connection): ConnectionSettings => ({ name: c.name, driver: c.driver, host: c.host, port: c.port, database: c.database, user: c.user, tls: { ...c.tls }, password: '', savePassword: !!c.hasPassword, ...(c.instance ? { instance: c.instance } : {}) });

// ---------------------------------------------------------------------------------------------------------------------------------
// column kinds and operators

export type Family = 'text' | 'number' | 'date';
/** A database's own type name → the family the operators are chosen by. (SQLite has no date type: its dates are texts, see `operatorsFor`.) */
export function familyOf(type: string): Family {
  const t = type.toLowerCase();
  if (/(date|time|year)/.test(t)) return 'date';
  if (/(int|serial|numeric|decimal|number|float|double|real|money|bit)/.test(t)) return 'number';
  return 'text';
}

export type OperatorId = '=' | '!=' | '<' | '<=' | '>' | '>=' | 'contains' | 'startsWith' | 'endsWith' | 'in' | 'between' | 'isEmpty' | 'isNotEmpty' | 'inMonth' | 'inDateRange';
export type ValueShape = 'none' | 'one' | 'list' | 'two' | 'month' | 'dates';
export const SHAPE: Record<OperatorId, ValueShape> = {
  '=': 'one', '!=': 'one', '<': 'one', '<=': 'one', '>': 'one', '>=': 'one', contains: 'one', startsWith: 'one', endsWith: 'one',
  in: 'list', between: 'two', isEmpty: 'none', isNotEmpty: 'none', inMonth: 'month', inDateRange: 'dates',
};
export const operatorLabel = (op: string): string => ({
  '=': tr('같음'), '!=': tr('다름'), '<': tr('보다 작음'), '<=': tr('작거나 같음'), '>': tr('보다 큼'), '>=': tr('크거나 같음'),
  contains: tr('포함'), startsWith: tr('시작'), endsWith: tr('끝'), in: tr('목록 중 하나'), between: tr('사이'),
  isEmpty: tr('비어 있음'), isNotEmpty: tr('비어 있지 않음'), inMonth: tr('이 달에 속함'), inDateRange: tr('기간 안'),
} as Record<string, string>)[op] ?? op;

/** The operators that fit a column: the ones for any column, then the text ones for a text, the date ones for a date (and for a text of SQLite, whose dates are texts). */
export function operatorsFor(type: string, driver: Driver): OperatorId[] {
  const family = familyOf(type);
  const common: OperatorId[] = ['=', '!=', '<', '<=', '>', '>=', 'in'];
  const tail: OperatorId[] = ['isEmpty', 'isNotEmpty'];
  if (family === 'text') return [...common, 'contains', 'startsWith', 'endsWith', ...(driver === 'sqlite' ? (['between', 'inMonth', 'inDateRange'] as OperatorId[]) : []), ...tail];
  if (family === 'date') return [...common, 'between', 'inMonth', 'inDateRange', ...tail];
  return [...common, 'between', ...tail];
}

/** The parameter types a condition value of this shape may point at. */
export const paramTypesFor = (shape: ValueShape): ParamType[] => shape === 'month' ? ['month'] : shape === 'list' ? ['list'] : shape === 'dates' || shape === 'two' ? [] : ['text', 'number', 'date', 'month'];

// ---------------------------------------------------------------------------------------------------------------------------------
// the condition tree: a root group (all / any) whose items are conditions or one more level of groups

export const isGroup = (n: FilterNode): n is Group => 'all' in n || 'any' in n;
export const groupKind = (g: Group): 'all' | 'any' => 'all' in g ? 'all' : 'any';
export const groupItems = (g: Group): FilterNode[] => 'all' in g ? g.all : g.any;
export const makeGroup = (kind: 'all' | 'any', items: FilterNode[]): Group => kind === 'all' ? { all: items } : { any: items };

export function newCondition(columns: ColumnInfo[], driver: Driver): Condition {
  const c = columns[0];
  return { column: c?.name ?? '', op: c ? operatorsFor(c.type, driver)[0] : '=', value: '' };
}
export const emptyFilter = (): Group => ({ all: [] });
/** The number of conditions in a tree. */
export const countConditions = (n: FilterNode | undefined): number => !n ? 0 : isGroup(n) ? groupItems(n).reduce((a, i) => a + countConditions(i), 0) : 1;
/** The tree without conditions that are not finished (no column, no value where one is needed): what the preview and the import send. */
export function finished(n: FilterNode, columns: ColumnInfo[]): FilterNode | null {
  if (isGroup(n)) {
    const items = groupItems(n).map((i) => finished(i, columns)).filter((i): i is FilterNode => !!i);
    return items.length ? makeGroup(groupKind(n), items) : null;
  }
  if (!n.column || !columns.some((c) => c.name === n.column)) return null;
  const shape = SHAPE[n.op as OperatorId];
  if (!shape) return null;
  const v = n.value;
  const blank = (x: unknown) => x === undefined || x === null || (typeof x === 'string' && x.trim() === '');
  const isParam = (x: unknown) => !!x && typeof x === 'object' && !Array.isArray(x) && typeof (x as { param?: unknown }).param === 'string' && (x as { param: string }).param !== '';
  if (shape === 'none') return { column: n.column, op: n.op };
  if (isParam(v)) return n;
  if (shape === 'one' || shape === 'month') return blank(v) ? null : n;
  if (shape === 'list') return Array.isArray(v) && v.some((x) => !blank(x)) ? { ...n, value: (v as unknown[]).filter((x) => !blank(x)) } : null;
  return Array.isArray(v) && v.length === 2 && !blank(v[0]) && !blank(v[1]) ? n : null;
}

// ---------------------------------------------------------------------------------------------------------------------------------
// the query

export function newQuery(connection: string, hint?: Hint): Query {
  return { connection, hint, columns: [], orderBy: [], params: [], refresh: 'manual' };
}
/** The query as the dialog edits it, with the unfinished conditions left out (a query the engine takes). */
export function cleanQuery(q: Query, columns: ColumnInfo[], advanced: boolean): Query {
  const out: Query = { ...q, columns: [...q.columns] };
  if (advanced) {
    delete out.table; delete out.filter; out.orderBy = []; out.columns = []; delete out.limit; delete out.offset;
    out.sql = q.sql ?? '';
    return out;
  }
  delete out.sql;
  const f = q.filter ? finished(q.filter, columns) : null;
  if (f && isGroup(f)) out.filter = f; else delete out.filter;
  out.orderBy = q.orderBy.filter((o) => o.column && columns.some((c) => c.name === o.column));
  if (!q.limit || q.limit < 1) delete out.limit;
  if (!q.offset || q.offset < 1) delete out.offset;
  return out;
}
/** Whether the query can be run: a connection and a table (or a statement). */
export const runnable = (q: Query, advanced: boolean): boolean => !!q.connection && (advanced ? !!q.sql?.trim() : !!q.table?.name);
export const REFRESH_MODES: Refresh[] = ['manual', 'askOnOpen', 'autoOnOpen'];
export const refreshLabel = (r: Refresh): string => r === 'askOnOpen' ? tr('문서를 열 때 묻기') : r === 'autoOnOpen' ? tr('문서를 열 때 자동으로') : tr('수동');

// ---------------------------------------------------------------------------------------------------------------------------------
// parameters

export const PARAM_TYPES: ParamType[] = ['text', 'number', 'date', 'month', 'list'];
export const paramTypeLabel = (t: ParamType): string => ({ text: tr('글'), number: tr('숫자'), date: tr('날짜'), month: tr('월'), list: tr('목록') })[t];
/** The relative values a date or a month parameter may have (the engine resolves them from today's date). */
export const RELATIVE: Record<'date' | 'month', { token: string; label: () => string }[]> = {
  date: [{ token: 'today', label: () => tr('오늘') }],
  month: [{ token: 'thisMonth', label: () => tr('이번 달') }, { token: 'lastMonth', label: () => tr('지난달') }, { token: 'nextMonth', label: () => tr('다음 달') }],
};
export const isRelative = (type: ParamType, v: unknown): boolean => (type === 'date' || type === 'month') && typeof v === 'string' && (RELATIVE[type].some((r) => r.token === v) || /^monthOffset:[+-]?\d+$/.test(v));
export function defaultFor(type: ParamType): unknown { return type === 'month' ? 'thisMonth' : type === 'date' ? 'today' : type === 'list' ? [] : ''; }
export function newParam(existing: Param[]): Param {
  let n = existing.length + 1;
  while (existing.some((p) => p.name === `p${n}`)) n++;
  // the first one is the month the issue is about (the common case), named "월" ("Month"); the user changes the name freely
  const month = tr('월');
  return { name: existing.some((p) => p.name === month) ? `p${n}` : month, type: 'month', default: 'thisMonth' };
}
/** A parameter name is letters, digits and `_` (the engine says the same). */
export const validParamName = (name: string): boolean => /^[\p{L}\p{N}_]+$/u.test(name);
export const paramText = (v: unknown): string => Array.isArray(v) ? v.join(', ') : v === undefined || v === null ? '' : String(v);

// ---------------------------------------------------------------------------------------------------------------------------------
// errors

/** The easy sentence for what went wrong (the engine's `errorKind`); the engine's own English sentence follows as the detail. */
export function explain(kind: string): string {
  switch (kind) {
    case 'needsPassword': return tr('이 연결에는 비밀번호가 필요합니다. 비밀번호를 입력해 잠금을 푸세요.');
    case 'auth': return tr('사용자 이름이나 비밀번호가 맞지 않습니다. 다시 확인해 주세요.');
    case 'network': return tr('데이터베이스 서버에 닿지 못했습니다. 주소와 포트, 네트워크(VPN·방화벽)를 확인해 주세요.');
    case 'tls': return tr('보안 연결(TLS)을 맺지 못했습니다. 서버의 인증서를 믿을 수 없거나 TLS 설정이 서로 맞지 않습니다.');
    case 'timeout': return tr('서버가 제때 답하지 않아 중단했습니다. 잠시 뒤 다시 시도하거나 조건을 좁혀 보세요.');
    case 'notFound': return tr('찾을 수 없습니다. 연결 이름이나 데이터베이스, 테이블, 열, SQLite 파일이 있는지 확인해 주세요.');
    case 'unsupported': return tr('이 데이터베이스 종류나 이 컴퓨터에서는 쓸 수 없습니다.');
    case 'invalid': return tr('입력한 값이나 질의가 올바르지 않습니다.');
    case 'sql': return tr('데이터베이스가 이 질의를 거절했습니다.');
    case 'limit': return tr('결과가 너무 큽니다(최대 50,000행). 조건을 더하거나 개수를 제한하세요.');
    case 'credentials': return tr('비밀번호 저장소를 쓸 수 없습니다. 비밀번호를 저장하지 않고 이 세션 동안만 기억하게 하세요.');
    default: return tr('알 수 없는 오류입니다.');
  }
}
export const isFailure = (e: unknown): e is { kind: string; message: string; fingerprint?: string } => !!e && typeof e === 'object' && 'kind' in e && 'message' in e;

/** `2026-11-19T10:20:30+09:00` → the local date and time, short. */
export function shortTime(iso: string | undefined): string {
  if (!iso) return '';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** Why passwords cannot be saved on this device (`dbCapabilities.credentialStore.reason`), in the screen's words. */
export function storeUnavailableText(reason?: string): string {
  switch (reason) {
    case 'secretToolMissing': return tr('이 컴퓨터에는 비밀번호 저장 도구(secret-tool, libsecret-tools 꾸러미)가 없어 비밀번호를 저장하지 않습니다. 입력한 비밀번호는 이 프로그램을 켜 둔 동안만 기억하고, 필요하면 그때 물어봅니다.');
    case 'androidNoStore': return tr('Android에서는 아직 비밀번호를 저장하지 않습니다(Android 키 저장소 연결이 아직 없습니다). 입력한 비밀번호는 이 프로그램을 켜 둔 동안만 기억하고, 필요하면 그때 물어봅니다.');
    default: return tr('이 컴퓨터에는 비밀번호 저장소가 없어 비밀번호를 저장하지 않습니다. 입력한 비밀번호는 이 프로그램을 켜 둔 동안만 기억하고, 필요하면 그때 물어봅니다.');
  }
}

/** The name of a credential store (`dbCapabilities.credentialStore.kind`). */
export function storeKindName(kind: string): string {
  switch (kind) {
    case 'windowsCredentialManager': return tr('Windows 자격 증명 관리자');
    case 'macosKeychain': return tr('macOS 키체인');
    case 'iosKeychain': return tr('iOS 키체인');
    case 'secretService': return tr('Secret Service (gnome-keyring, KWallet 등)');
    case 'memory': return tr('시험용 메모리 저장소');
    default: return tr('없음');
  }
}

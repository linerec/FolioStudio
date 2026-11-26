// Data Merge ▸ a database as the data source: what the screen asks the engine (dbmerge DESIGN 2). Every answer is the engine's: the SQL is made
// there from the query definition (the screen never builds or filters SQL). A password is only ever sent to the engine, never read back: it is in the OS
// credential store (`hasPassword`) or kept in the engine's memory for this session (`unlocked`, `dbUnlockConnection`); no query or edit command carries it
// except the connection test of settings that are not saved yet and `dbUnlockConnection` itself.
export type Driver = 'postgres' | 'mysql' | 'sqlite' | 'sqlserver';
export type TlsMode = 'verify' | 'require' | 'disable';
export type Tls = { mode: TlsMode; trustFingerprint?: string };
/** A saved connection (user settings of this computer; no password). */
export type Connection = { name: string; driver: Driver; host: string; port?: number; database: string; user: string; tls: Tls; hasPassword?: boolean; unlocked?: boolean;
  /** SQL Server: a named instance (`SQLEXPRESS`); without a port the engine asks the SQL Server Browser for it. */
  instance?: string };
/** What the document remembers of a connection: no user name, no password. */
export type Hint = { driver: Driver; host: string; port?: number; database: string; instance?: string };
/** The settings of a connection as the form sends them (`dbSaveConnection`, `dbTestConnection`). */
export type ConnectionSettings = Omit<Connection, 'hasPassword' | 'unlocked'> & { password?: string; savePassword?: boolean };

export type TableInfo = { schema: string | null; name: string; kind: 'table' | 'view' };
export type ColumnInfo = { name: string; type: string; nullable: boolean };
export type TableRef = { schema?: string | null; name: string };

export type Condition = { column: string; op: string; value?: unknown };
/** `{all:[…]}` or `{any:[…]}`; the items are conditions or one more level of groups (two levels at most). */
export type Group = { all: FilterNode[] } | { any: FilterNode[] };
export type FilterNode = Condition | Group;
export type Order = { column: string; dir: 'asc' | 'desc' };
export type ParamType = 'text' | 'number' | 'date' | 'month' | 'list';
export type Param = { name: string; type: ParamType; default?: unknown };
export type Refresh = 'manual' | 'askOnOpen' | 'autoOnOpen';

/** The query definition the document keeps (label `folio.datamerge`, key `database.query`). */
export type Query = {
  connection: string;
  hint?: Hint;
  table?: TableRef;
  columns: string[];
  filter?: Group;
  orderBy: Order[];
  limit?: number;
  offset?: number;
  sql?: string;
  params: Param[];
  refresh: Refresh;
};

/** An error the engine gives: `errorKind` is stable (the screen explains it), `error` is the engine's English sentence. */
export type DbFailure = { errorKind: string; error: string; fingerprint?: string };
export class DbError extends Error {
  kind: string;
  fingerprint?: string;
  constructor(failure: DbFailure) {
    super(failure.error);
    this.kind = failure.errorKind || 'unknown';
    this.fingerprint = failure.fingerprint;
  }
}

async function ask<T>(body: Record<string, unknown>): Promise<T> {
  const { header } = await window.folio.request(body);
  const h = header as unknown as { ok?: boolean; error?: unknown; errorKind?: string; fingerprint?: string };
  if (!h.ok) throw new DbError({ errorKind: h.errorKind ?? 'unknown', error: String(h.error ?? `${String(body.cmd)} failed`), fingerprint: h.fingerprint });
  return header as unknown as T;
}

export const dbConnections = () => ask<{ connections: Connection[]; credentialStore: boolean }>({ cmd: 'dbConnections' });
export const saveConnection = (c: ConnectionSettings) => ask<{ ok: true }>({ cmd: 'dbSaveConnection', ...c });
export const removeConnection = (name: string) => ask<{ ok: true }>({ cmd: 'dbRemoveConnection', name });
/** A connection already saved (by name), or settings not saved yet (with the password just typed). */
export const testConnection = (c: string | ConnectionSettings) =>
  ask<{ serverVersion: string; ms: number }>(typeof c === 'string' ? { cmd: 'dbTestConnection', connection: c } : { cmd: 'dbTestConnection', connection: c, password: c.password });
/**
 * Unlock a connection whose password was not saved for this session: the engine checks the password against the server and keeps a right one in its
 * memory (`save`: in the OS credential store too). A wrong one is refused (`auth`) and not kept.
 */
export const unlockConnection = (name: string, password: string, save = false) => ask<{ unlocked: boolean; saved: boolean }>({ cmd: 'dbUnlockConnection', name, password, save });
/** Forget the password this session kept for a connection. */
export const lockConnection = (name: string) => ask<{ ok: true }>({ cmd: 'dbLockConnection', name });
/** The tables and columns use the session's password, then the saved one; neither: `needsPassword`. */
export const dbTables = (connection: string) => ask<{ tables: TableInfo[] }>({ cmd: 'dbTables', connection });
export const dbColumns = (connection: string, table: TableRef) => ask<{ columns: ColumnInfo[] }>({ cmd: 'dbColumns', connection, table });

export type Preview = { fields: { name: string; kind: string }[]; columns: string[]; records: (string | null)[][]; count: number | null; sql: string; params: string[]; ms: number; paramValues: Record<string, unknown> };
export const dbPreview = (query: Query, paramValues?: Record<string, unknown>, limit?: number) => ask<Preview>({ cmd: 'dbPreview', query, paramValues, limit });

export type SourceStatus =
  | { kind: 'none' }
  | { kind: 'file'; source?: string }
  | { kind: 'database'; connection: string; connectionPresent: boolean; hint?: Hint; fetchedAt?: string; rows?: number; fields?: unknown; paramValues?: Record<string, unknown>;
      lastRefresh?: { rows: number; added: number; removed: number; changed: number } | null; refresh?: Refresh; query: Query; changed?: boolean; checkError?: { errorKind: string; error: string } };
/** The state of the document's data source; with `check` the query is run again and its result compared with the snapshot (`changed`). */
export const sourceStatus = (check = false) => ask<SourceStatus>({ cmd: 'dataMergeSourceStatus', check });

/** The values of the computed fields (mergeexpr-c1) of the document for one row, computed by the engine. */
export async function computedValues(fields: { name: string; expr: string }[], record: Record<string, string>): Promise<string[]> {
  const r = await ask<{ fields: { name: string; value?: string }[] }>({ cmd: 'dataMergeEvaluate', fields, record });
  return r.fields.map((f) => f.value ?? '');
}

/** What this device's credential store is (`dbCapabilities.credentialStore`); `reason` says why passwords cannot be saved here. */
export type CredentialStoreInfo = { available: boolean; kind: string; reason?: string; message?: string };
/** What this device can do with databases (dbplatform-c1): passwords in the OS store or the session only, certificate checking. */
export type DbCapabilities = { platform: string; credentialStore: CredentialStoreInfo; sessionPasswords: boolean; tlsVerify: { available: boolean; reason?: string; message?: string } };
export const dbCapabilities = () => ask<DbCapabilities>({ cmd: 'dbCapabilities' });
/** One step of the credential store's self test: `save`, `read`, `remove`, `gone`. */
export type SelfTestStep = { step: string; ok: boolean; errorKind?: string; error?: string };
export type SelfTestResult = { ok: boolean; store: CredentialStoreInfo; steps: SelfTestStep[]; ms: number };
/** Help > Diagnostics: keep, read and remove a throwaway entry in this computer's credential store (no connection is touched). A failed test is a result, not an error. */
export async function credentialStoreSelfTest(): Promise<SelfTestResult> {
  const { header } = await window.folio.request({ cmd: 'dbCredentialStoreSelfTest' });
  const h = header as unknown as Partial<SelfTestResult> & { error?: unknown };
  if (!h.store || !Array.isArray(h.steps)) throw new DbError({ errorKind: 'unknown', error: String(h.error ?? 'dbCredentialStoreSelfTest failed') });
  return { ok: !!h.ok, store: h.store, steps: h.steps, ms: h.ms ?? 0 };
}

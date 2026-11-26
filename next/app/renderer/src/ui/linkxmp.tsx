// Links panel ▸ Utilities ▸ XMP File Info… and Versions…: read-only windows about the file of the picked link (the XMP packet the file
// carries; its size and modified time). Nothing is written to the document or the file.
import { useEffect, useState } from 'react';
import { getFileMetadata, type FileMetadata, type LinkRow } from '../engine';
import { tr } from '../i18n';
import { Modal } from './tables';

/** The engine's reading of a link's file (`fileMetadata`): undefined while it reads, null when the file cannot be read. */
export function useFileMetadata(file: string | null | undefined): FileMetadata | null | undefined {
  const [facts, setFacts] = useState<FileMetadata | null | undefined>(undefined);
  useEffect(() => {
    let live = true;
    setFacts(undefined);
    (file ? getFileMetadata(file) : Promise.resolve(null)).then((f) => { if (live) setFacts(f); }, () => { if (live) setFacts(null); });
    return () => { live = false; };
  }, [file]);
  return facts;
}

/** The XMP properties the Links panel shows for a link, as label and value (only the ones the file says). */
export function linkXmpRows(facts: FileMetadata | null | undefined): [string, string][] {
  const f = facts?.fields;
  if (!f) return [];
  const rows: [string, string][] = [
    [tr('제목:'), f.title], [tr('설명:'), f.description], [tr('작성자:'), f.creator.join('; ')], [tr('저작권:'), f.rights],
    [tr('키워드:'), f.keywords.join('; ')], [tr('헤드라인:'), f.headline], [tr('크레디트:'), f.credit], [tr('출처:'), f.source],
  ];
  return rows.filter(([, v]) => v.trim().length > 0);
}

export type XmpField = { name: string; values: string[] };
const RDF = 'http://www.w3.org/1999/02/22-rdf-syntax-ns#';

/** The properties of an XMP packet as a list: attributes and child elements of its descriptions; arrays (Alt, Bag, Seq) give their items,
 *  anything nested is shown as its text. */
export function xmpFields(packet: string): XmpField[] {
  const doc = new DOMParser().parseFromString(packet, 'application/xml');
  if (doc.querySelector('parsererror')) return [];
  const out: XmpField[] = [];
  const text = (e: Element) => (e.textContent ?? '').replace(/\s+/g, ' ').trim();
  for (const d of Array.from(doc.getElementsByTagNameNS(RDF, 'Description'))) {
    for (const a of Array.from(d.attributes)) {
      if (a.name.startsWith('xmlns') || a.name === 'rdf:about') continue;
      out.push({ name: a.name, values: [a.value] });
    }
    for (const c of Array.from(d.children)) {
      const items = Array.from(c.getElementsByTagNameNS(RDF, 'li')).map(text).filter(Boolean);
      const values = items.length ? items : [text(c)].filter(Boolean);
      out.push({ name: c.prefix ? `${c.prefix}:${c.localName}` : c.localName, values });
    }
  }
  return out;
}

const when = (ms: number) => new Date(ms).toLocaleString();
const sizeText = (n: number) => (n < 1024 ? `${n} B` : n < 1048576 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1048576).toFixed(1)} MB`);

export function LinkFileDialog({ kind, link, close }: { kind: 'xmp' | 'versions'; link: LinkRow; close: () => void }) {
  const facts = useFileMetadata(link.path);
  const fields = facts?.packet ? xmpFields(facts.packet) : [];
  return (
    <Modal title={kind === 'xmp' ? tr('XMP 파일 정보') : tr('버전')} close={close} ok submit={() => undefined}>
      <fieldset><legend>{tr('파일')}</legend>
        <p data-link-file-name>{link.name}</p>
        {facts === undefined ? <p className="p-empty small">{tr('읽는 중…')}</p>
          : facts === null ? <p className="p-empty small" role="alert">{tr('파일을 읽을 수 없습니다. 링크가 끊겼거나 포함되어 있습니다.')}</p>
          : kind === 'xmp' ? (
            fields.length ? (
              <table data-xmp-fields><tbody>{fields.map((f, i) => <tr key={i}><th scope="row">{f.name}</th><td>{f.values.join(', ')}</td></tr>)}</tbody></table>
            ) : <p className="p-empty small" data-xmp-none>{facts.packet ? tr('읽을 수 있는 속성이 없습니다.') : tr('이 파일에는 XMP 메타데이터가 없습니다.')}</p>
          ) : (
            <table data-link-versions><tbody>
              <tr><th scope="row">{tr('현재 버전')}</th><td>{facts.modified !== null ? when(facts.modified) : ''}</td></tr>
              <tr><th scope="row">{tr('크기')}</th><td>{sizeText(facts.size)}</td></tr>
              <tr><th scope="row">{tr('상태')}</th><td>{link.status}</td></tr>
            </tbody></table>
          )}
        {kind === 'versions' && facts && <p className="p-empty small">{tr('버전 서버(Version Cue·Creative Cloud)에 연결되어 있지 않아 파일의 현재 버전만 보여 줍니다.')}</p>}
      </fieldset>
    </Modal>
  );
}

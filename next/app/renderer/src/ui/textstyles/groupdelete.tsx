// Delete Style Group (Paragraph Styles and Character Styles panels): the group goes with every style in it. What follows is told here
// before it happens (the engine's rules: text/format/styles/group_manage.rs, oracle text-format tsg-delete-*).
import { t as tr } from '../../i18n';
import { createPortal } from 'react-dom';

export function GroupDeleteDialog({ name, count, onOk, close }: { name: string; count: number; onOk: () => void; close: () => void }) {
  return createPortal(
    <div className="style-modal" role="dialog" aria-label={tr('스타일 그룹 삭제')} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
      <form className="card dialog style-options" onSubmit={(e) => { e.preventDefault(); onOk(); close(); }}>
        <h2>{tr('스타일 그룹 삭제')}</h2>
        <p data-group-delete-note="">{tr('스타일 그룹 "{0}"과 그 안의 스타일 {1}개를 지웁니다.', { 0: name, 1: count })}</p>
        <p>{tr('이 스타일을 쓰던 글은 스타일에서 떨어져 나오고 모양은 그대로 남습니다. 이 스타일을 기준으로 삼던 스타일은 기준이 없어지고 설정을 그대로 가져갑니다.')}</p>
        <div className="dialog-actions"><button type="button" onClick={close}>{tr('취소')}</button><button type="submit" autoFocus>{tr('삭제')}</button></div>
      </form>
    </div>,
    document.body,
  );
}

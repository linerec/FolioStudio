import type { Command, Commands } from '../../ui/commands';
import type { PanelId, useDock } from '../../ui/dock';
import { CharacterPanel, ParagraphPanel, TransformPanel, StrokePanel, type PanelCtx } from '../../ui/panels';
import { tr } from '../../i18n';
import { PathfinderPanel } from '../../ui/pathfinderpanel';
import { AttributesPanel, BackgroundTasksPanel, ScriptLabelPanel, ToolHintsPanel } from '../../ui/utilitypanels';
import { TabsPanel } from '../../ui/tabspanel';
import { TablePanel } from '../../ui/tablepanel';
import { ConditionsPanel } from '../conditions/panel';
import { IndexPanel } from '../indexes/panel';
import { TrapPresetsPanel } from '../traps/panel';
import { FlattenerPreviewPanel } from '../flattener/panel';
import { GlyphsPanel } from '../glyphs/panel';
import { DataMergePanel } from '../datamerge/panel';
import { PdfCommentsPanel } from '../pdfcomments/panel';
import { MathPanel } from '../math/panel';

function Properties({ ctx, commands }: { ctx: PanelCtx; commands: Commands }) {
  if (!ctx.doc) return <p className="p-empty">{tr('문서가 열려 있지 않습니다.')}</p>;
  return <div className="properties-panel">
    {ctx.selection.length ? <>
      <h3>{tr('변형')}</h3><TransformPanel ctx={ctx} />
      <h3>{tr('획')}</h3><StrokePanel ctx={ctx} />
      {ctx.textFormatting.selection && <><h3>{tr('문자')}</h3><CharacterPanel ctx={ctx} /><h3>{tr('단락')}</h3><ParagraphPanel ctx={ctx} /></>}
    </> : <div className="p-empty">
      <p>{tr('페이지 수: {0}', { 0: ctx.doc.pages.length })}</p>
      <button disabled={!commands['file.documentSetup']?.run} onClick={() => commands['file.documentSetup']?.run?.()}>{tr('문서 설정…')}</button>
      <p>{tr('개체를 선택하면 위치, 크기와 획을 편집할 수 있습니다.')}</p>
    </div>}
  </div>;
}

const PANELS = {
  properties: { command: 'id.window.properties', render: (ctx: PanelCtx, commands: Commands) => <Properties ctx={ctx} commands={commands} /> },
  pathfinder: { command: 'id.window.objectAndLayout.pathfinder', render: (_ctx: PanelCtx, commands: Commands) => <PathfinderPanel commands={commands} /> },
  attributes: { command: 'id.window.output.attributes', render: (ctx: PanelCtx) => <AttributesPanel ctx={ctx} /> },
  scriptLabel: { command: 'id.window.utilities.scriptLabel', render: (ctx: PanelCtx) => <ScriptLabelPanel ctx={ctx} /> },
  toolHints: { command: 'id.window.utilities.toolHints', render: (ctx: PanelCtx) => <ToolHintsPanel tool={ctx.tool} /> },
  backgroundTasks: { command: 'id.window.utilities.backgroundTasks', render: (ctx: PanelCtx) => <BackgroundTasksPanel ctx={ctx} /> },
  tabs: { command: 'type.tabs', render: (ctx: PanelCtx, commands: Commands) => <TabsPanel ctx={ctx} commands={commands} /> },
  table: { command: 'id.window.typeAndTables.table', render: (ctx: PanelCtx) => <TablePanel ctx={ctx} /> },
  index: { command:'id.window.typeAndTables.index', render:(ctx:PanelCtx)=><IndexPanel ctx={ctx}/> },
  conditionalText: { command:'id.window.typeAndTables.conditionalText', render:(ctx:PanelCtx)=><ConditionsPanel ctx={ctx}/> },
  trapPresets: { command:'id.window.output.trapPresets', render:(ctx:PanelCtx)=><TrapPresetsPanel ctx={ctx}/> },
  flattenerPreview: { command:'id.window.output.flattenerPreview', render:(ctx:PanelCtx)=><FlattenerPreviewPanel ctx={ctx}/> },
  glyphs: { command:'type.glyphs', render:(ctx:PanelCtx,commands:Commands)=><GlyphsPanel ctx={ctx} commands={commands}/> },
  pdfComments: { command:'id.window.comments.pdfComments', render:(ctx:PanelCtx)=><PdfCommentsPanel ctx={ctx}/> },
  dataMerge: { command:'id.window.utilities.dataMerge', render:(ctx:PanelCtx)=><DataMergePanel ctx={ctx}/> },
  mathExpressions: { command:'id.window.mathExpressions', render:(ctx:PanelCtx)=><MathPanel ctx={ctx}/> },
};

export function windowCommands(dock: ReturnType<typeof useDock>): Record<string, Partial<Command>> {
  return Object.fromEntries(Object.entries(PANELS).map(([id, p]) => [p.command, {
    run: () => dock.show(id as PanelId), checked: dock.isOpen(id as PanelId),
  }]));
}

export function windowPanel(id: PanelId, ctx: PanelCtx, commands: Commands) {
  return PANELS[id as keyof typeof PANELS]?.render(ctx, commands);
}

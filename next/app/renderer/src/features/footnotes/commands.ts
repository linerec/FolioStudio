import type { Command } from '../../ui/commands';
import type { TextActions, TextSelectionState } from '../../ui/textedit';

export function footnoteCommands(editable: boolean, text: TextSelectionState | null, actions: React.MutableRefObject<TextActions | null>, options: () => void, convert: () => void): Record<string, Partial<Command>> {
  return {
    'id.type.insertFootnote': { run: editable && text && !text.nested && text.start === text.end ? () => actions.current?.footnote() : undefined },
    'id.type.documentFootnoteOptions': { run: editable ? options : undefined },
    // measured: enabled with a text insertion point or a text range, not with nothing or a frame selected
    'id.type.insertEndnote': { run: editable && text && !text.nested ? () => actions.current?.endnote() : undefined },
    'id.type.convertFootnotesAndEndnotes': { run: editable && text && !text.nested ? convert : undefined },
    'id.type.insertSpecialCharacter.markers.footnoteNumber': { run: editable && text?.noteContext?.canInsertFootnoteMarker && text.start === text.end ? () => actions.current?.special('FOOTNOTE_SYMBOL') : undefined },
    'id.ctx.text.goToFootnoteText': { run: editable && text?.noteContext?.footnoteTarget && text.start === text.end ? () => actions.current?.goToFootnote() : undefined },
    'id.ctx.text.goToEndnoteText': { run: editable && text?.noteContext?.endnoteTarget && text.start === text.end ? () => actions.current?.goToEndnote() : undefined },
  };
}

import type {EditCommand} from '../../engine';
import {timestamp} from './types';

export function authored(commands:EditCommand[],user:string):EditCommand[] {
  const date=timestamp();
  return commands.map(c=>c.op==='insertText'?{...c,op:'insertTextAs',user,date}:
    c.op==='deleteText'?{...c,op:'deleteTextAs',user,date}:
    c.op==='deleteNote'?{...c,op:'deleteNoteAs',user,date}:
    c.op==='noteToText'?{...c,op:'noteToTextAs',user,date}:
    c.op==='removeNotes'?{...c,op:'removeNotesAs',user,date}:c);
}

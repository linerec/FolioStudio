// Equations (MathML): what the screen asks the engine. The layout is the engine's (query `mathmlLayout`, crate folio-math); the dialog and its
// live preview are the shared dialog frame's `insertMath` / `editMath` (`dialogPreview`). Inserting and changing an equation are edit commands
// (`insertMathml`, `setMathml`), the list of the document's equations is the query `mathExpressions`.
export type Rgb = [number, number, number];

/** One equation of the document (`mathExpressions`). */
export type MathExpression = { id: string; page: string; mathml: string; size: number; color: Rgb; width: number; height: number };

async function ask(body: Record<string, unknown>): Promise<Record<string, unknown>> {
  const { header } = await window.folio.request(body);
  if (!header.ok) throw new Error(String((header as { error?: unknown }).error ?? `${String(body.cmd)} failed`));
  return header as unknown as Record<string, unknown>;
}

/** The equations of the open document, in document order; empty when there is no document or the engine cannot say. */
export async function listMathExpressions(): Promise<MathExpression[]> {
  try {
    const r = await ask({ cmd: 'mathExpressions' });
    return Array.isArray(r.items) ? (r.items as MathExpression[]) : [];
  } catch {
    return [];
  }
}

/** The text of a MathML source on one line (what the panel lists): the tags dropped, the white space folded. */
export function mathSummary(mathml: string): string {
  const text = mathml.replace(/<[^>]*>/g, ' ').replace(/&[a-zA-Z0-9#]+;/g, '·').replace(/\s+/g, ' ').trim();
  return text || mathml.replace(/\s+/g, ' ').trim();
}

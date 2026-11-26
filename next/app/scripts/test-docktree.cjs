// Part of test-dock.cjs (run with node next/app/scripts/test-dock.cjs): the dock areas (left, right = `column`, bottom) and their
// split trees, edge drops, maximize, shares, area sizes, floating panels that remember their home, snapping (uix-d1, 2026-11-20).
const assert = require('node:assert/strict');

module.exports = function (m, { env, run, eq, copy }) {
  const { CLASSIC, applyDock, normalizeDock, describeDock, whereIs, areaTree, snapRect, AREA_SIZE } = m;
  const groups = (s) => [...s.strip, ...s.column, ...(s.left ?? []), ...(s.bottom ?? [])];
  const flat = (s) => [...groups(s).flat(), ...s.floating.flatMap((f) => f.panels)];
  function sound(s, msg) {
    const all = flat(s);
    assert.equal(new Set(all).size, all.length, `${msg}: no panel twice`);
    for (const g of groups(s)) { assert.ok(g.length, `${msg}: no empty group`); assert.ok(g.includes(s.active[g[0]]), `${msg}: a tab shows in ${g[0]}`); }
    // every area's tree names each of its groups once
    for (const a of ['left', 'column', 'bottom']) {
      const list = a === 'column' ? s.column : s[a] ?? [];
      const t = areaTree(s, a);
      const seen = [];
      const walk = (n) => { if (n.kind === 'group') seen.push(n.group); else { assert.ok(Math.abs(n.sizes.reduce((x, y) => x + y, 0) - 1) < 1e-3, `${msg}: ${a} shares sum to 1`); n.parts.forEach(walk); } };
      if (t) walk(t);
      eq(seen, list.map((_, i) => i), `${msg}: ${a} tree in reading order`);
    }
    // a layout that holds reads back as itself
    eq(normalizeDock(copy(s), env), s, `${msg}: repair keeps a sound layout`);
  }
  const tree = (s, a) => { const t = areaTree(s, a); const w = (n) => (n.kind === 'group' ? (a === 'column' ? s.column : s[a])[n.group].join('+') : `${n.dir}(${n.parts.map(w).join(' | ')})`); return t ? w(t) : ''; };

  // an older layout (no areas, no tree) reads back exactly as it was: the new fields stay out while they hold their defaults
  eq(Object.keys(normalizeDock(CLASSIC, env)).sort(), ['active', 'column', 'columnCollapsed', 'floating', 'folded', 'strip'], 'no new fields for the classic layout');

  // split beside a group: right of the project panel (the right area becomes two columns side by side)
  let s = run(CLASSIC, { op: 'movePanel', panel: 'layers', to: { zone: 'column', group: 0, edge: 'right' } });
  assert.equal(tree(s, 'column'), 'row(project | layers)');
  eq(s.column, [['project'], ['layers']], 'the groups in reading order');
  eq(s.layout.column, { dir: 'row', parts: [0, 1], sizes: [0.5, 0.5] });
  assert.equal(s.size.column, AREA_SIZE.column.def * 2, 'a second column widens the area by a column');
  sound(s, 'split right');
  // below layers: layers' column splits; project keeps its whole height
  s = run(s, { op: 'movePanel', panel: 'links', to: { zone: 'column', group: 1, edge: 'bottom' } });
  assert.equal(tree(s, 'column'), 'row(project | col(layers | links))');
  sound(s, 'split below');
  // above project, in the outer column: project's place splits too
  s = run(s, { op: 'movePanel', panel: 'info', to: { zone: 'column', group: 0, edge: 'top' } });
  assert.equal(tree(s, 'column'), 'row(col(info | project) | col(layers | links))');
  eq(s.column, [['info'], ['project'], ['layers'], ['links']]);
  sound(s, 'split above');
  // the old commands still name the groups by their number in reading order
  s = run(s, { op: 'movePanel', panel: 'color', to: { zone: 'column', group: 2 } });
  eq(s.column[2], ['layers', 'color'], 'join group 2 (layers)');
  assert.equal(describeDock(s).find((r) => r.panel === 'color').shown, true);
  s = run(s, { op: 'movePanel', panel: 'stroke', to: { zone: 'column', before: 1 } });
  assert.equal(tree(s, 'column'), 'row(col(info | stroke | project) | col(layers+color | links))', 'before group 1: above project in its column');
  sound(s, 'old commands on a tree');
  // a split loses a part: a split of one part goes away, a split inside one of the same direction merges into it
  s = run(s, { op: 'closePanel', panel: 'links' });
  assert.equal(tree(s, 'column'), 'row(col(info | stroke | project) | layers+color)');
  s = run(s, { op: 'moveGroup', zone: 'column', group: 3, to: { zone: 'column', group: 1, edge: 'bottom' } });
  assert.equal(tree(s, 'column'), 'col(info | stroke | layers+color | project)', 'the row is gone: one column');
  assert.equal(s.size, undefined, 'and the area is one column wide again');
  sound(s, 'collapse splits');

  // the outer edge of an area: a new column across the whole area, which widens the area
  s = run(CLASSIC, { op: 'movePanel', panel: 'layers', to: { zone: 'column', before: 1 } });
  s = run(s, { op: 'movePanel', panel: 'info', to: { zone: 'column', edge: 'left' } });
  assert.equal(tree(s, 'column'), 'row(info | col(project | layers))');
  assert.equal(s.size.column, AREA_SIZE.column.def * 2, 'a new dock column widens the area');
  sound(s, 'area edge');
  // the left and bottom areas start empty; any edge of an empty area takes the panel
  s = run(s, { op: 'movePanel', panel: 'pages', to: { zone: 'left', edge: 'top' } });
  eq(s.left, [['pages']]);
  s = run(s, { op: 'movePanel', panel: 'history', to: { zone: 'bottom', before: 0 } });
  s = run(s, { op: 'movePanel', panel: 'character', to: { zone: 'bottom', before: 1 } });
  assert.equal(tree(s, 'bottom'), 'row(history | character)', 'the bottom area lies side by side');
  assert.equal(s.layout?.bottom, undefined, 'side by side with equal shares is the plain bottom: no tree saved');
  eq(whereIs(s, 'character'), { zone: 'bottom', group: 1, index: 0 });
  sound(s, 'left and bottom');
  // a whole group to another area keeps its tabs and the one showing
  s = run(run(CLASSIC, { op: 'setActive', panel: 'links' }), { op: 'moveGroup', zone: 'strip', group: 0, to: { zone: 'left', before: 0 } });
  eq(s.left, [['pages', 'layers', 'links']]);
  assert.equal(s.active.pages, 'links');
  sound(s, 'group to the left');

  // shares: a drag sets them (no part below the smallest share), a double click evens them
  s = run(CLASSIC, { op: 'movePanel', panel: 'layers', to: { zone: 'column', before: 1 } });
  s = run(s, { op: 'resizeSplit', zone: 'column', path: [], sizes: [3, 1] });
  eq(s.layout.column, { dir: 'col', parts: [0, 1], sizes: [0.75, 0.25] }, 'uneven shares are saved as a tree');
  s = run(s, { op: 'resizeSplit', zone: 'column', path: [], sizes: [1, 0] });
  assert.ok(s.layout.column.sizes[1] >= 0.05, 'a part keeps the smallest share');
  s = run(s, { op: 'equalizeSplit', zone: 'column', path: [] });
  assert.equal(s.layout, undefined, 'even shares again: the plain column');
  let r = applyDock(s, { op: 'resizeSplit', zone: 'column', path: [], sizes: [1] }, env);
  assert.ok(!r.ok && r.error === 'invalid', 'one share for two parts');
  r = applyDock(s, { op: 'resizeSplit', zone: 'column', path: [4], sizes: [1, 1] }, env);
  assert.ok(!r.ok && r.error === 'badGroup');
  // area size: bounds, and the document keeps its room
  s = run(CLASSIC, { op: 'setAreaSize', zone: 'column', size: 5000 });
  assert.equal(s.size.column, AREA_SIZE.column.max);
  s = run(CLASSIC, { op: 'setAreaSize', zone: 'column', size: 700, view: { w: 900, h: 700 } });
  assert.ok(s.size.column <= 900 - 320 - 160, 'the document keeps 320 px (and the strip its room)');
  s = run(s, { op: 'setAreaSize', zone: 'column', size: AREA_SIZE.column.def });
  assert.equal(s.size, undefined, 'the default size is not saved');

  // maximize: the group has its area; showing another group of the area gives the others back
  s = run(CLASSIC, { op: 'movePanel', panel: 'layers', to: { zone: 'column', before: 1 } });
  s = run(s, { op: 'maximizeGroup', zone: 'column', group: 1, on: true });
  assert.equal(s.maximized, 'layers');
  s = run(s, { op: 'openPanel', panel: 'project' });
  assert.equal(s.maximized, undefined, 'opening another group of the area ends it');
  s = run(run(s, { op: 'maximizeGroup', zone: 'column', group: 1, on: true }), { op: 'maximizeGroup', zone: 'column', group: 1, on: false });
  assert.equal(s.maximized, undefined);
  // folding works in every area
  s = run(run(CLASSIC, { op: 'movePanel', panel: 'info', to: { zone: 'left', edge: 'top' } }), { op: 'foldGroup', zone: 'left', group: 0, folded: true });
  eq(copy(s.folded), { info: true });
  s = run(s, { op: 'collapseArea', zone: 'left', collapsed: true });
  eq(s.collapsed, { left: true });
  s = run(s, { op: 'collapseArea', zone: 'column', collapsed: true });
  assert.equal(s.columnCollapsed, true, 'the right area is the old collapseColumn');
  s = run(s, { op: 'openPanel', panel: 'info' });
  assert.equal(s.collapsed, undefined, 'showing a panel of a folded area unfolds it');
  assert.equal(s.folded.info, undefined, 'and its group');

  // floating: a group floats and remembers its place; a double click on its title puts it back
  s = run(CLASSIC, { op: 'floatGroup', zone: 'strip', group: 1, x: 400, y: 300 });
  eq(s.floating, [{ panels: ['stroke', 'color'], active: 'stroke', x: 400, y: 300, w: 250, h: 300, home: { zone: 'strip', group: 1 } }]);
  s = run(s, { op: 'dockFloating', group: 0 });
  eq(s.strip, CLASSIC.strip, 'back between the same groups');
  eq(s.floating, []);
  // one tab floats out; back home it joins the group it left
  s = run(CLASSIC, { op: 'movePanel', panel: 'layers', to: { zone: 'floating', x: 500, y: 200 } });
  s = run(s, { op: 'moveFloating', group: 0, x: 520, y: 210, w: 400, h: 200 });
  eq([s.floating[0].x, s.floating[0].y, s.floating[0].w, s.floating[0].h], [520, 210, 400, 200]);
  s = run(s, { op: 'dockFloating', group: 0 });
  eq(s.strip[0], ['pages', 'links', 'layers'], 'layers is back with pages and links');
  // a floating panel dropped on another one joins its tabs
  s = run(CLASSIC, { op: 'openPanel', panel: 'info' });
  s = run(s, { op: 'openPanel', panel: 'align' });
  eq(s.floating.map((f) => f.panels), [['info'], ['align']], 'closed panels open floating');
  s = run(s, { op: 'moveGroup', zone: 'floating', group: 1, to: { zone: 'floating', group: 0 } });
  eq(s.floating.map((f) => f.panels), [['info', 'align']]);
  // the window got smaller: every floating panel comes back onto it, no bigger than it
  s = run(s, { op: 'moveFloating', group: 0, x: 3000, y: 2000 });
  s = run(s, { op: 'fitFloating', view: { w: 800, h: 600 } });
  const f = s.floating[0];
  assert.ok(f.x + Math.min(f.w, 120) <= 800 && f.y <= 560 && f.w <= 800 && f.h <= 568, `on the screen: ${JSON.stringify(f)}`);
  // closing a whole group
  s = run(CLASSIC, { op: 'closeGroup', zone: 'strip', group: 0 });
  eq(s.strip, [['stroke', 'color'], ['swatches']]);

  // repair of trees: unknown groups, a tree naming a group twice or not at all, shares that do not add up
  s = normalizeDock({
    strip: [], column: [['project'], ['bogus'], ['layers'], ['links']], active: {}, columnCollapsed: false, floating: [],
    layout: { column: { dir: 'row', parts: [0, { dir: 'col', parts: [1, 2, 2], sizes: [5, 5, 5] }], sizes: [2, 2] } },
    left: [['info']], bottom: [], size: { left: 9, column: 'x' }, collapsed: { bottom: true, left: 'yes' }, maximized: 'nonesuch',
  }, env);
  assert.equal(tree(s, 'column'), 'col(row(project | layers) | links)', 'the empty group is gone, the split of one part merged, the group the tree left out comes below');
  eq(s.left, [['info']]);
  eq(s.size, { left: AREA_SIZE.left.min }, 'sizes within bounds');
  eq(s.collapsed, { bottom: true });
  assert.equal(s.maximized, undefined);
  sound(s, 'repaired tree');
  // a bad tree is no tree: the plain arrangement
  s = normalizeDock({ strip: [], column: [['project'], ['layers']], active: {}, columnCollapsed: false, floating: [], layout: { column: { dir: 'diagonal', parts: [0, 1] } } }, env);
  assert.equal(s.layout, undefined);
  eq(s.column, [['project'], ['layers']]);
  // errors are results
  for (const [cmd, kind] of [
    [{ op: 'movePanel', panel: 'info', to: { zone: 'column', group: 7, edge: 'left' } }, 'badTarget'],
    [{ op: 'movePanel', panel: 'info', to: { zone: 'strip', edge: 'left' } }, 'badTarget'],
    [{ op: 'movePanel', panel: 'info', to: { zone: 'column', edge: 'middle' } }, 'badTarget'],
    [{ op: 'movePanel', panel: 'info', to: { zone: 'moon', before: 0 } }, 'badTarget'],
    [{ op: 'maximizeGroup', zone: 'left', group: 0, on: true }, 'badGroup'],
    [{ op: 'dockFloating', group: 0 }, 'badGroup'],
    [{ op: 'setAreaSize', zone: 'strip', size: 100 }, 'badTarget'],
    [{ op: 'fitFloating' }, 'invalid'],
  ]) {
    const res = applyDock(CLASSIC, cmd, env);
    assert.ok(!res.ok && res.error === kind, `${JSON.stringify(cmd)} -> ${res.ok ? 'ok' : res.error}`);
  }

  // snapping: near another floating panel's edge or the window's, the rectangle jumps onto it; far away it stays
  eq(snapRect({ x: 205, y: 100, w: 100, h: 100 }, [{ x: 0, y: 100, w: 200, h: 100 }], { w: 1000, h: 800 }), { x: 200, y: 100, w: 100, h: 100 }, 'to the right edge of the other');
  eq(snapRect({ x: 895, y: 3, w: 100, h: 100 }, [], { w: 1000, h: 800 }), { x: 900, y: 0, w: 100, h: 100 }, 'to the window edges');
  eq(snapRect({ x: 500, y: 300, w: 100, h: 100 }, [], { w: 1000, h: 800 }), { x: 500, y: 300, w: 100, h: 100 }, 'nothing near');
  eq(snapRect({ x: 100, y: 100, w: 95, h: 100 }, [{ x: 200, y: 0, w: 10, h: 10 }], { w: 1000, h: 800 }, { r: true }), { x: 100, y: 100, w: 100, h: 100 }, 'a resized edge snaps alone');

  // uix-d2: a floating group out to an operating-system window of its own (another monitor), moved there, and back
  {
    let s = run(CLASSIC, { op: 'openPanel', panel: 'info' });
    s = run(s, { op: 'popOut', zone: 'floating', group: 0, window: { x: -1600, y: 120, w: 300, h: 260 } });
    eq(s.floating[0].window, { x: -1600, y: 120, w: 300, h: 260 }, 'popOut keeps the desktop place (a monitor on the left: negative x)');
    eq(normalizeDock(copy(s), env).floating[0].window, s.floating[0].window, 'the window place reads back');
    sound(s, 'popped out');
    s = run(s, { op: 'moveWindow', group: 0, window: { x: 40, y: 50, w: 320, h: 280 } });
    eq(s.floating[0].window.x, 40, 'moved by hand');
    assert.ok(describeDock(s).find((r) => r.panel === 'info').window, 'described as in a window');
    assert.equal(run(s, { op: 'popIn', group: 0 }).floating[0].window, undefined, 'popIn: back inside the app window');
    const docked = run(CLASSIC, { op: 'popOut', zone: 'strip', group: 1, window: { x: 10, y: 10, w: 250, h: 330 } });
    eq(docked.floating[0].panels, ['stroke', 'color'], 'a docked group floats first');
    eq(docked.floating[0].home, { zone: 'strip', group: 1 }, 'and remembers its home');
    eq(run(docked, { op: 'dockFloating', group: 0 }).strip[1], ['stroke', 'color'], 'closing its window docks it back');
    const one = run(CLASSIC, { op: 'openPanel', panel: 'info' });
    for (const bad of [{ op: 'popOut', zone: 'floating', group: 0, window: { x: 0, y: 0, w: 10, h: 10 } }, { op: 'moveWindow', group: 0, window: { x: 1, y: 1, w: 200, h: 200 } }, { op: 'popOut', zone: 'column', group: 5 }]) {
      assert.ok(!applyDock(one, bad, env).ok, `refused: ${JSON.stringify(bad)}`);
    }
  }
  // uix-d2: a group of panels the workspace leaves out (the Folio views in InDesign Classic) is no column: the area does not widen for it
  {
    const s = run(CLASSIC, { op: 'movePanel', panel: 'info', to: { zone: 'left', edge: 'left' } });
    const shown = applyDock(s, { op: 'movePanel', panel: 'layers', to: { zone: 'left', edge: 'left' } }, env);
    const left = applyDock(s, { op: 'movePanel', panel: 'layers', to: { zone: 'left', edge: 'left' } }, { ...env, hidden: (id) => id === 'info' });
    eq(shown.state.size, { left: AREA_SIZE.left.def * 2 }, 'two columns shown: twice as wide');
    assert.equal(left.state.size, undefined, 'one of them left out: as wide as one');
  }
  console.log('dock trees ok');
};

// Runs axe-core (WCAG 2.2 AA) over sites generated from tests/fixtures/sample,
// tests/fixtures/variables and tests/fixtures/nested, for every layout and
// color theme, and exits non-zero on any violation.
//
//   node audit.js <sample-site> <variables-site> <nested-site> [--quick] [--json FILE]
//
// --quick checks light and dark only (the CI matrix); the default also
// checks slate. Each page is audited as loaded, again with every <details>
// and nav list opened, and again with the search results open.
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');
const { pathToFileURL } = require('url');

const TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];
const LAYOUTS = ['classic', 'minimal', 'modern'];

const args = process.argv.slice(2);
const quick = args.includes('--quick');
const jsonAt = args.indexOf('--json');
const jsonFile = jsonAt >= 0 ? args[jsonAt + 1] : null;
const positional = args.filter((a, i) => !a.startsWith('--') && !(jsonAt >= 0 && i === jsonAt + 1));
const [sampleSite, variablesSite, nestedSite] = positional;
if (!sampleSite || !variablesSite || !nestedSite) {
  console.error('usage: node audit.js <sample-site> <variables-site> <nested-site> [--quick] [--json FILE]');
  process.exit(2);
}
const THEMES = quick ? ['light', 'dark'] : ['light', 'dark', 'slate'];

// One page per page type, plus a module with variables and one with nested
// classes (recursive class cards, a nested outline, class badges).
const PAGES = [
  { site: sampleSite, page: 'index.html', kind: 'index', query: 'engine' },
  { site: sampleSite, page: '01_core/engine.html', kind: 'module', query: 'status' },
  { site: sampleSite, page: 'guide.html', kind: 'document', query: 'slug' },
  { site: variablesSite, page: 'config.html', kind: 'module (variables)', query: 'mode' },
  { site: nestedSite, page: 'compat.html', kind: 'module (nested classes)', query: 'deep' },
];

const axeSource = fs.readFileSync(require.resolve('axe-core/axe.min.js'), 'utf8');

async function audit(page) {
  await page.addScriptTag({ content: axeSource });
  return page.evaluate((tags) => window.axe.run(document, { runOnly: { type: 'tag', values: tags }, resultTypes: ['violations'] }), TAGS);
}

(async () => {
  const browser = await chromium.launch();
  const byRule = new Map();
  let runs = 0;
  for (const layout of LAYOUTS) {
    for (const theme of THEMES) {
      const context = await browser.newContext({ reducedMotion: 'reduce', viewport: { width: 1280, height: 800 } });
      await context.addInitScript(([l, t]) => {
        try { localStorage.setItem('layout', l); localStorage.setItem('theme', t); } catch (e) {}
      }, [layout, theme]);
      const page = await context.newPage();
      for (const target of PAGES) {
        await page.goto(pathToFileURL(path.join(path.resolve(target.site), target.page)).href);
        await page.waitForTimeout(250);
        const attrs = await page.evaluate(() => [document.documentElement.dataset.layout, document.documentElement.dataset.theme]);
        if (attrs[0] !== layout || attrs[1] !== theme) throw new Error(`expected ${layout}/${theme}, page shows ${attrs.join('/')}`);

        for (const state of ['loaded', 'expanded', 'search open']) {
          if (state === 'expanded') {
            // Controls inside closed <details> and collapsed nav lists are
            // not rendered, so axe only measures them once they are open.
            await page.evaluate(() => {
              document.querySelectorAll('details').forEach((d) => { d.open = true; });
              document.querySelectorAll('.nav-chevron[aria-expanded="false"]').forEach((b) => b.click());
            });
          }
          if (state === 'search open') {
            await page.keyboard.press('/');
            await page.keyboard.type(target.query);
            await page.waitForSelector('#results-list .search-item', { timeout: 10000 });
          }
          const result = await audit(page);
          runs++;
          for (const v of result.violations) {
            const entry = byRule.get(v.id) || { id: v.id, impact: v.impact, help: v.help, nodes: 0, where: new Set(), examples: new Map(), all: [] };
            entry.nodes += v.nodes.length;
            entry.where.add(`${layout}/${theme} ${target.kind}${state === 'loaded' ? '' : ' +search'}`);
            for (const node of v.nodes) {
              const key = node.target.join(' ');
              if (!entry.examples.has(key)) entry.examples.set(key, (node.failureSummary || '').split('\n').slice(1).join(' ').trim());
              if (jsonFile) entry.all.push({ layout, theme, page: target.kind, state, target: key, html: node.html.slice(0, 120), summary: node.failureSummary });
            }
            byRule.set(v.id, entry);
          }
        }
      }
      await context.close();
    }
  }
  await browser.close();

  const rules = [...byRule.values()].sort((a, b) => b.nodes - a.nodes);
  console.log(`axe-core ${require('axe-core/package.json').version}, tags ${TAGS.join(', ')}: ${runs} audits (${LAYOUTS.length} layouts x ${THEMES.length} themes x ${PAGES.length} pages x 3 states)`);
  if (rules.length === 0) {
    console.log('No violations.');
  }
  for (const r of rules) {
    console.log(`\n${r.id} (${r.impact}): ${r.help}`);
    console.log(`  ${r.nodes} nodes in ${r.where.size} of ${runs} audits`);
    for (const [target, why] of [...r.examples].slice(0, 8)) console.log(`  - ${target}: ${why.slice(0, 160)}`);
    if (r.examples.size > 8) console.log(`  ... ${r.examples.size - 8} more distinct elements`);
  }
  if (jsonFile) {
    fs.writeFileSync(jsonFile, JSON.stringify(rules.map((r) => ({ ...r, where: [...r.where], examples: [...r.examples] })), null, 1));
  }
  process.exit(rules.length ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(2); });

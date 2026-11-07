// Landmarks and headings: one <main>, every <nav> labelled, a single <h1>,
// and no skipped heading levels.
//
//   node structure.js <sample-site> <variables-site> <nested-site>
const { chromium } = require('playwright');
const path = require('path');
const { pathToFileURL } = require('url');

const [sampleSite, variablesSite, nestedSite] = process.argv.slice(2);
if (!sampleSite || !variablesSite || !nestedSite) {
  console.error('usage: node structure.js <sample-site> <variables-site> <nested-site>');
  process.exit(2);
}
const PAGES = [
  [sampleSite, 'index.html'],
  [sampleSite, '01_core/engine.html'],
  [sampleSite, '01_core/index.html'],
  [sampleSite, 'utils/helpers.html'],
  [sampleSite, 'guide.html'],
  [variablesSite, 'config.html'],
  [variablesSite, 'links.html'],
  [nestedSite, 'compat.html'],
];

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage();
  let failures = 0;
  for (const [site, file] of PAGES) {
    await page.goto(pathToFileURL(path.join(path.resolve(site), file)).href);
    const info = await page.evaluate(() => ({
      mains: document.querySelectorAll('main, [role="main"]').length,
      navs: Array.from(document.querySelectorAll('nav, [role="navigation"]')).map((n) => n.getAttribute('aria-label') || n.getAttribute('aria-labelledby') || ''),
      headings: Array.from(document.querySelectorAll('h1, h2, h3, h4, h5, h6'))
        .filter((h) => h.offsetParent !== null || getComputedStyle(h).position === 'fixed')
        .map((h) => ({ level: Number(h.tagName[1]), text: h.textContent.trim().slice(0, 40) })),
    }));
    const problems = [];
    if (info.mains !== 1) problems.push(`${info.mains} <main> elements`);
    info.navs.forEach((label, i) => { if (!label) problems.push(`<nav> #${i + 1} has no label`); });
    const h1s = info.headings.filter((h) => h.level === 1).length;
    if (h1s !== 1) problems.push(`${h1s} <h1> elements`);
    let previous = 0;
    for (const h of info.headings) {
      if (h.level > previous + 1) problems.push(`h${h.level} "${h.text}" follows h${previous || '-'}`);
      previous = h.level;
    }
    const outline = info.headings.map((h) => `h${h.level}`).join(' ');
    console.log(`${problems.length ? 'FAIL' : 'OK  '} ${path.basename(site)}/${file}: main=${info.mains} nav=[${info.navs.join(', ')}] ${outline}`);
    problems.forEach((p) => console.log(`       ${p}`));
    failures += problems.length;
  }
  await browser.close();
  process.exit(failures ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(2); });

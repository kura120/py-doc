// Keyboard-only checks of the interactive parts of a generated site: the
// search combobox and its listbox, the nav tree's expand/collapse buttons,
// the theme toggle, and the layout and color theme selects.
//
//   node keyboard.js <sample-site>
const { chromium } = require('playwright');
const path = require('path');
const { pathToFileURL } = require('url');

const site = process.argv[2];
if (!site) {
  console.error('usage: node keyboard.js <sample-site>');
  process.exit(2);
}
const url = (file) => pathToFileURL(path.join(path.resolve(site), file)).href;

let failures = 0;
function check(cond, message) {
  console.log(`${cond ? 'OK  ' : 'FAIL'} ${message}`);
  if (!cond) failures++;
}

// Presses Tab until `matches` holds for the focused element.
async function tabTo(page, matches, what, limit = 80, arg = null) {
  for (let i = 0; i < limit; i++) {
    await page.keyboard.press('Tab');
    if (await page.evaluate(matches, arg)) return i + 1;
  }
  throw new Error(`Tab never reached ${what}`);
}

const focused = (page) => page.evaluate(() => {
  const el = document.activeElement;
  return { id: el.id, tag: el.tagName, cls: el.className, label: el.getAttribute('aria-label'), expanded: el.getAttribute('aria-expanded') };
});
const searchState = (page) => page.evaluate(() => {
  const input = document.getElementById('search-input');
  const active = input.getAttribute('aria-activedescendant');
  const option = active && document.getElementById(active);
  return {
    role: input.getAttribute('role'),
    expanded: input.getAttribute('aria-expanded'),
    controls: input.getAttribute('aria-controls'),
    listRole: document.getElementById(input.getAttribute('aria-controls') || 'x')?.getAttribute('role'),
    active,
    activeSelected: option ? option.getAttribute('aria-selected') : null,
    activeInList: !!(option && option.closest('[role="listbox"]')),
    activeHref: option ? option.getAttribute('href') : null,
    options: document.querySelectorAll('#results-list [role="option"]').length,
    open: document.getElementById('search-results').classList.contains('is-open'),
    value: input.value,
    focusIsInput: document.activeElement === input,
  };
});

(async () => {
  const browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const page = await context.newPage();

  // --- Search ---
  console.log('Search (combobox + listbox):');
  await page.goto(url('01_core/engine.html'));
  const tabs = await tabTo(page, () => document.activeElement.id === 'search-input', 'the search box');
  check(true, `the search box is reached with Tab (${tabs} presses)`);
  let s = await searchState(page);
  check(s.role === 'combobox' && s.expanded === 'false' && s.controls === 'results-list' && s.listRole === 'listbox',
    `the input is a combobox controlling a listbox, collapsed (role=${s.role}, aria-expanded=${s.expanded}, controls=${s.controls} -> ${s.listRole})`);
  await page.keyboard.type('engine');
  await page.waitForSelector('#results-list [role="option"]');
  s = await searchState(page);
  check(s.open && s.expanded === 'true' && s.options > 1, `typing opens the listbox (aria-expanded=${s.expanded}, ${s.options} options)`);
  check(s.active === 'search-option-0' && s.activeSelected === 'true' && s.activeInList,
    `the first option is active via aria-activedescendant (${s.active}, aria-selected=${s.activeSelected})`);
  check(s.focusIsInput, 'focus stays in the input');
  await page.keyboard.press('ArrowDown');
  s = await searchState(page);
  check(s.active === 'search-option-1' && s.activeSelected === 'true', `ArrowDown moves to the next option (${s.active})`);
  const unselected = await page.evaluate(() => document.getElementById('search-option-0').getAttribute('aria-selected'));
  check(unselected === 'false', 'the previous option is no longer selected');
  await page.keyboard.press('ArrowUp');
  s = await searchState(page);
  check(s.active === 'search-option-0', `ArrowUp moves back (${s.active})`);
  await page.keyboard.press('ArrowUp');
  s = await searchState(page);
  check(s.active === `search-option-${s.options - 1}`, `ArrowUp from the first option wraps to the last (${s.active})`);
  await page.keyboard.press('Escape');
  s = await searchState(page);
  check(!s.open && s.expanded === 'false' && !s.active && s.focusIsInput && s.value === 'engine',
    `Esc closes the results and keeps focus and text in the input (open=${s.open}, focus in input=${s.focusIsInput}, value="${s.value}")`);
  await page.keyboard.press('ArrowDown');
  s = await searchState(page);
  check(s.open && s.active === 'search-option-0', 'ArrowDown reopens the results');
  await page.keyboard.press('Escape');
  await page.keyboard.press('Escape');
  s = await searchState(page);
  check(s.value === '' && s.focusIsInput, 'a second Esc clears the box, focus still in it');
  await page.keyboard.type('status');
  await page.waitForSelector('#results-list [role="option"]');
  s = await searchState(page);
  const target = new URL(s.activeHref, page.url()).href;
  await Promise.all([page.waitForURL(target), page.keyboard.press('Enter')]);
  check(page.url() === target, `Enter opens the active option (${path.basename(new URL(target).pathname)}${new URL(target).hash})`);
  const status = await page.evaluate(() => document.getElementById('search-status')?.getAttribute('role'));
  check(status === 'status', 'result counts and messages go to a role="status" line');

  // "/" from the page body.
  await page.goto(url('01_core/engine.html'));
  await page.keyboard.press('/');
  check((await focused(page)).id === 'search-input', '"/" moves focus to the search box');

  // --- Nav tree ---
  console.log('Nav tree (disclosure buttons):');
  await page.goto(url('index.html'));
  await tabTo(page, () => document.activeElement.classList.contains('nav-chevron'), 'a nav expand/collapse button');
  let f = await focused(page);
  check(f.tag === 'BUTTON' && !!f.label && f.expanded !== null, `the first nav button is reachable with Tab: "${f.label}", aria-expanded=${f.expanded}`);
  const listHidden = () => page.evaluate(() => document.getElementById(document.activeElement.getAttribute('aria-controls')).hidden);
  const before = f.expanded;
  check((await listHidden()) === (before === 'false'), 'aria-expanded matches the controlled list');
  await page.keyboard.press('Enter');
  f = await focused(page);
  check(f.expanded !== before && (await listHidden()) === (f.expanded === 'false'), `Enter toggles it (aria-expanded ${before} -> ${f.expanded})`);
  await page.keyboard.press('Space');
  f = await focused(page);
  check(f.expanded === before && (await listHidden()) === (before === 'false'), `Space toggles it back (aria-expanded -> ${f.expanded})`);
  // A collapsed module row: open it and Tab into its symbols.
  await page.goto(url('index.html'));
  await tabTo(page, () => {
    const el = document.activeElement;
    return el.classList.contains('nav-chevron') && el.closest('.nav-group') && el.getAttribute('aria-expanded') === 'false';
  }, 'a collapsed module row');
  f = await focused(page);
  await page.keyboard.press('Enter');
  await page.keyboard.press('Tab');
  const intoList = await page.evaluate(() => document.activeElement.classList.contains('nav-item-link'));
  check(intoList, `after opening "${f.label}", Tab moves into its symbol links`);
  // Collapsed lists are skipped by Tab.
  const hiddenLinksFocusable = await page.evaluate(() => Array.from(document.querySelectorAll('[hidden] a')).some((a) => a.offsetParent !== null));
  check(!hiddenLinksFocusable, 'links in collapsed lists are not shown, so Tab skips them');

  // --- Theme toggle and selects ---
  console.log('Theme and layout controls:');
  await page.goto(url('index.html'));
  const theme = () => page.evaluate(() => document.documentElement.getAttribute('data-theme'));
  const layout = () => page.evaluate(() => document.documentElement.getAttribute('data-layout'));
  await tabTo(page, () => document.activeElement.id === 'theme-toggle', 'the theme toggle');
  const t0 = await theme();
  await page.keyboard.press('Enter');
  const t1 = await theme();
  check(t0 !== t1, `the theme toggle works with Enter (${t0} -> ${t1})`);
  await page.keyboard.press('Space');
  check((await theme()) === t0, `and with Space (back to ${t0})`);

  for (const [id, label, read] of [['layout-select', 'Layout', layout], ['theme-select', 'Color theme', theme]]) {
    const name = await page.evaluate((i) => document.getElementById(i).labels[0]?.textContent.trim(), id);
    check(name === label, `#${id} is labelled "${name}"`);
    const reached = await tabTo(page, (i) => document.activeElement.id === i, `#${id}`, 120, id).then(() => true, () => false);
    check(reached, `#${id} is reached with Tab`);
    if (!reached) await page.focus(`#${id}`);
    const v0 = await read();
    await page.keyboard.press('ArrowDown');
    await page.waitForTimeout(100);
    const v1 = await read();
    check(v0 !== v1, `#${id} changes with ArrowDown (${v0} -> ${v1})`);
  }
  // The sidebar <nav> is labelled, and the select accessible names resolve.
  const byLabel = await page.getByLabel('Layout', { exact: true }).count() + await page.getByLabel('Color theme', { exact: true }).count();
  check(byLabel === 2, 'both selects are found by their label');

  await browser.close();
  console.log(failures ? `\n${failures} FAILURES` : '\nALL KEYBOARD CHECKS PASSED');
  process.exit(failures ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(2); });

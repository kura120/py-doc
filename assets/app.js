// --- Icon set (inline SVG, themed per data-layout via CSS custom properties) ---
// One shape language shared by all three layouts; Classic/Minimal/Modern each
// restyle stroke weight, color and (Modern) a chip background purely in CSS —
// see the ".nav-icon svg" rules in style.css. Keeping one source of truth here
// avoids duplicating three icon libraries.
const ICONS = {
    folder: `<svg viewBox="0 0 20 20" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><path d="M2.5 5.5C2.5 4.67 3.17 4 4 4h3.5L9 5.5h7c.83 0 1.5.67 1.5 1.5v7c0 .83-.67 1.5-1.5 1.5H4c-.83 0-1.5-.67-1.5-1.5v-8.5Z" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/></svg>`,
    file: `<svg viewBox="0 0 20 20" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><path d="M5 3h6l4 4v10a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1Z" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/><path d="M11 3v4h4" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/></svg>`,
    chevron: `<svg class="chevron-icon" viewBox="0 0 12 12" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><path d="M4 2.5 8 6l-4 3.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>`,
    search: `<svg viewBox="0 0 20 20" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><circle cx="8.5" cy="8.5" r="5.5" stroke="currentColor" stroke-width="1.4"/><path d="M16 16l-3.8-3.8" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/></svg>`,
};

// --- Nav tree rendering (built client-side from nav-data.js) ---
function escapeHtml(str) {
    return String(str)
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;");
}

function slugifyLinkPath(linkPath) {
    return linkPath.replace(/\//g, "-").replace(/\./g, "-");
}

function renderSymbolItems(targetUrl, classes, functions, tagName, tagClass) {
    const openTag = tagClass ? `<${tagName} class="${tagClass}">` : `<${tagName}>`;
    let html = "";
    (classes || []).forEach(cls => {
        html += `${openTag}<a class="nav-item-link" href="${targetUrl}#class.${encodeURIComponent(cls.name)}">class ${escapeHtml(cls.name)}</a></${tagName}>`;
    });
    (functions || []).forEach(fn => {
        html += `${openTag}<a class="nav-item-link" href="${targetUrl}#fn.${encodeURIComponent(fn.name)}">def ${escapeHtml(fn.name)}</a></${tagName}>`;
    });
    return html;
}

function renderModuleEntry(mod, rootPath, currentPath) {
    const targetUrl = rootPath + mod.link_path;
    const isActive = !!currentPath && mod.link_path === currentPath;
    const slug = slugifyLinkPath(mod.link_path);
    const hasChildren = (mod.classes && mod.classes.length) || (mod.functions && mod.functions.length);

    if (!hasChildren) {
        return `<div class="nav-group-flat nav-module ${isActive ? "active-mod" : ""}" id="nav-group-${slug}">
            <a class="nav-module-link" href="${targetUrl}"><span class="nav-icon">${ICONS.file}</span>${escapeHtml(mod.name)}</a>
        </div>`;
    }

    const name = escapeHtml(mod.name);
    return `<div class="nav-group${isActive ? " is-open" : ""}" id="nav-group-${slug}">
        <div class="nav-header ${isActive ? "active-mod" : ""}">
            <a href="${targetUrl}"${isActive ? ' aria-current="page"' : ""}><span class="nav-icon">${ICONS.file}</span>${name}</a>
            ${disclosureButton(`nav-items-${slug}`, isActive, `Symbols in ${name}`)}
        </div>
        <ul id="nav-items-${slug}"${isActive ? "" : " hidden"}>${renderSymbolItems(targetUrl, mod.classes, mod.functions, "li", null)}</ul>
    </div>`;
}

// The expand/collapse control of a nav row: a real button, so it is
// reachable with Tab and works with Enter and Space, and screen readers
// hear whether the list is open.
function disclosureButton(controls, expanded, label) {
    return `<button type="button" class="nav-chevron" aria-expanded="${expanded ? "true" : "false"}" aria-controls="${controls}" aria-label="${label}">${ICONS.chevron}</button>`;
}

// Folder groups arrive flat, named by path ("core", "core/sub"). Nest each
// one under its parent folder when that parent is itself a group; otherwise
// it stays at the top level under its full path.
function nestNavGroups(navData) {
    const byName = new Map();
    const nodes = (navData || []).map(group => {
        const node = Object.assign({}, group, { children: [], label: group.name });
        if (group.name) byName.set(group.name, node);
        return node;
    });

    const roots = [];
    nodes.forEach(node => {
        const cut = node.name ? node.name.lastIndexOf("/") : -1;
        const parent = cut > 0 ? byName.get(node.name.slice(0, cut)) : null;
        if (parent) {
            node.label = node.name.slice(cut + 1);
            parent.children.push(node);
        } else {
            roots.push(node);
        }
    });
    return roots;
}

function renderNavGroup(group, rootPath, currentPath) {
    // Ungrouped bucket (root-level modules with no folder): just a flat run of entries
    if (!group.name) {
        return (group.modules || []).map(mod => renderModuleEntry(mod, rootPath, currentPath)).join("");
    }

    const folderUrl = group.link_path ? (rootPath + group.link_path) : null;
    const folderIsActive = !!folderUrl && !!currentPath && group.link_path === currentPath;
    const label = escapeHtml(group.label || group.name);

    const headerInner = folderUrl
        ? `<a href="${folderUrl}"${folderIsActive ? ' aria-current="page"' : ""}><span class="nav-icon">${ICONS.folder}</span>${label}</a>`
        : `<span><span class="nav-icon">${ICONS.folder}</span>${label}</span>`;
    const contentsId = `nav-folder-${slugifyLinkPath(group.name)}`;

    const symbolsInner = folderUrl
        ? renderSymbolItems(folderUrl, group.classes, group.functions, "div", "nav-group-flat")
        : "";

    const modulesInner = (group.modules || []).map(mod => renderModuleEntry(mod, rootPath, currentPath)).join("");
    const childrenInner = (group.children || []).map(child => renderNavGroup(child, rootPath, currentPath)).join("");

    return `<div class="nav-folder is-open">
        <div class="folder-header ${folderIsActive ? "active-mod" : ""}">${headerInner}${disclosureButton(contentsId, true, `${label} folder`)}</div>
        <div class="folder-contents" id="${contentsId}">${symbolsInner}${modulesInner}${childrenInner}</div>
    </div>`;
}

function renderNavTree(navData, rootPath, currentPath) {
    return nestNavGroups(navData).map(group => renderNavGroup(group, rootPath, currentPath)).join("");
}

// --- Search (names and one-line summaries from the search index) ---
const SEARCH_LIMIT = 40;

// The index is loaded on the first search, not with every page. Over HTTP
// it is fetched as JSON. Browsers block fetch() of local files, so on
// file:// (or if the fetch fails) search-index.js is added as a script,
// which defines the global `searchIndex`. A page whose template still loads
// search-index.js itself already has it.
let searchIndexLoad = null;

function loadSearchIndex(rootPath) {
    if (searchIndexLoad) return searchIndexLoad;
    const loaded = () => (typeof searchIndex !== "undefined" ? searchIndex : null);
    const injectScript = () => loaded() ? Promise.resolve(loaded()) : new Promise((resolve, reject) => {
        const script = document.createElement("script");
        script.src = `${rootPath}search-index.js`;
        script.onload = () => (loaded() ? resolve(loaded()) : reject(new Error("search-index.js defined no index")));
        script.onerror = () => reject(new Error("could not load search-index.js"));
        document.head.appendChild(script);
    });
    if (loaded()) {
        searchIndexLoad = Promise.resolve(loaded());
    } else if (location.protocol === "file:" || typeof fetch !== "function") {
        searchIndexLoad = injectScript();
    } else {
        searchIndexLoad = fetch(`${rootPath}search-index.json`)
            .then(response => {
                if (!response.ok) throw new Error(`HTTP ${response.status}`);
                return response.json();
            })
            .catch(injectScript);
    }
    // A failed load can be retried on the next search.
    searchIndexLoad.catch(() => { searchIndexLoad = null; });
    return searchIndexLoad;
}

function collectSearchEntries(index) {
    const entries = [];
    (index.modules || []).forEach(mod => {
        entries.push({ type: "Module", name: mod.name, summary: mod.summary, link: mod.link_path });
        (mod.classes || []).forEach(cls => {
            entries.push({ type: "Class", name: `${mod.name}.${cls.name}`, summary: cls.summary, link: `${mod.link_path}#class.${cls.name}` });
            (cls.functions || []).forEach(f => {
                entries.push({ type: "Method", name: `${mod.name}.${cls.name}.${f.name}`, summary: f.summary, link: `${mod.link_path}#method.${cls.name}.${f.name}` });
            });
            (cls.variables || []).forEach(v => {
                entries.push({ type: "Attribute", name: `${mod.name}.${cls.name}.${v.name}`, summary: v.summary, link: `${mod.link_path}#attr.${cls.name}.${v.name}` });
            });
        });
        (mod.functions || []).forEach(f => {
            entries.push({ type: "Function", name: `${mod.name}.${f.name}`, summary: f.summary, link: `${mod.link_path}#fn.${f.name}` });
        });
        (mod.variables || []).forEach(v => {
            entries.push({ type: "Variable", name: `${mod.name}.${v.name}`, summary: v.summary, link: `${mod.link_path}#var.${v.name}` });
        });
    });
    return entries;
}

// Escapes `text` and wraps every occurrence of `query` (case-insensitive) in <mark>.
function highlightMatch(text, query) {
    const source = String(text || "");
    const lower = source.toLowerCase();
    let html = "";
    let from = 0;
    while (query) {
        const at = lower.indexOf(query, from);
        if (at === -1) break;
        html += escapeHtml(source.slice(from, at)) + `<mark>${escapeHtml(source.slice(at, at + query.length))}</mark>`;
        from = at + query.length;
    }
    return html + escapeHtml(source.slice(from));
}

function findMatches(entries, query) {
    const inName = [];
    const inSummary = [];
    entries.forEach(entry => {
        const name = entry.name.toLowerCase();
        if (name.includes(query)) {
            // Matches on the last segment (the symbol itself) rank first.
            const leaf = name.slice(name.lastIndexOf(".") + 1);
            inName.push({ entry, rank: leaf.startsWith(query) ? 0 : leaf.includes(query) ? 1 : 2 });
        } else if (entry.summary && entry.summary.toLowerCase().includes(query)) {
            inSummary.push({ entry, rank: 3 });
        }
    });
    inName.sort((a, b) => a.rank - b.rank || a.entry.name.length - b.entry.name.length);
    return inName.concat(inSummary).map(match => match.entry);
}

document.addEventListener("DOMContentLoaded", () => {
    setupVersionSwitcher();
    const root = document.documentElement;

    // --- Layout (Classic / Minimal / Modern) + color theme ---
    // The theme *preference* is auto/light/dark/slate; "auto" follows the
    // reader's system setting. data-theme always holds the resolved value.
    const layoutSelect = document.getElementById("layout-select");
    const themeSelect = document.getElementById("theme-select");
    const themeToggle = document.getElementById("theme-toggle");
    const systemDark = window.matchMedia("(prefers-color-scheme: dark)");

    function stored(key) {
        try { return localStorage.getItem(key); } catch (e) { return null; }
    }
    function store(key, value) {
        try { localStorage.setItem(key, value); } catch (e) { /* private mode */ }
    }

    // Fall back to whatever the generator baked in as the site's default
    // (set via --layout/--theme at generation time), not a hardcoded value.
    const bakedLayout = root.getAttribute("data-layout") || "classic";
    const bakedTheme = root.getAttribute("data-theme-pref") || root.getAttribute("data-theme") || "auto";

    let themePref = stored("theme") || bakedTheme;

    function resolveTheme(pref) {
        return pref === "auto" ? (systemDark.matches ? "dark" : "light") : pref;
    }

    function applyTheme() {
        root.setAttribute("data-theme-pref", themePref);
        root.setAttribute("data-theme", resolveTheme(themePref));
        if (themeSelect) themeSelect.value = themePref;
    }

    function setThemePref(pref) {
        themePref = pref;
        store("theme", pref);
        applyTheme();
    }

    const savedLayout = stored("layout") || bakedLayout;
    root.setAttribute("data-layout", savedLayout);
    applyTheme();

    systemDark.addEventListener("change", () => {
        if (themePref === "auto") applyTheme();
    });

    if (themeSelect) {
        themeSelect.addEventListener("change", (e) => setThemePref(e.target.value));
    }
    if (themeToggle) {
        themeToggle.addEventListener("click", () => {
            setThemePref(resolveTheme(themePref) === "light" ? "dark" : "light");
        });
    }

    // --- Nav tree: render from the shared nav-data.js payload, and re-render
    // whenever the layout changes (icon markup is shared, but the collapse
    // state bindings below need to be re-attached to the fresh DOM nodes).
    const navContainer = document.getElementById("nav-tree");
    const rootPath = (navContainer && navContainer.dataset.rootPath) || "./";

    // Opens or closes a nav row (a module's symbols or a folder's contents).
    function setNavOpen(group, open) {
        const button = group.querySelector(":scope > .nav-header > .nav-chevron, :scope > .folder-header > .nav-chevron");
        if (!button) return;
        const list = document.getElementById(button.getAttribute("aria-controls"));
        button.setAttribute("aria-expanded", open ? "true" : "false");
        group.classList.toggle("is-open", open);
        if (list) list.hidden = !open;
    }

    function bindCollapseState() {
        // Module rows remember whether the reader left them open.
        document.querySelectorAll(".nav-group[id]").forEach(group => {
            const savedState = stored(group.id);
            if (savedState !== null) setNavOpen(group, savedState === "open");
        });
        navContainer.addEventListener("click", (e) => {
            const header = e.target.closest(".nav-header, .folder-header");
            // The chevron button, or a click on the row outside its link.
            if (!header || (e.target.closest("a") && !e.target.closest(".nav-chevron"))) return;
            const group = header.parentElement;
            const open = !group.classList.contains("is-open");
            setNavOpen(group, open);
            if (group.classList.contains("nav-group") && group.id) store(group.id, open ? "open" : "closed");
        });
    }

    function renderNav() {
        if (navContainer && typeof navData !== "undefined") {
            const currentPath = navContainer.dataset.currentPath || "";
            navContainer.innerHTML = renderNavTree(navData, rootPath, currentPath);
            bindCollapseState();
            updateActiveSidebarItem();
        }
    }

    // Marks the sidebar and outline links for the symbol being read: the
    // one the scroll spy reports, or else the one in the URL hash. Links are
    // compared as resolved URLs, so the "../" prefix on nested pages is irrelevant.
    let spiedId = null;

    function updateActiveSidebarItem() {
        const hash = spiedId ? `#${spiedId}` : decodeURIComponent(window.location.hash);
        const here = window.location.pathname + hash;
        document.querySelectorAll(".nav-item-link, .page-outline a").forEach(link => {
            const target = link.pathname + decodeURIComponent(link.hash);
            const isMatch = !!link.hash && target === here;
            link.classList.toggle("active-item", isMatch);
            if (isMatch) {
                const group = link.closest(".nav-group, .nav-folder");
                if (group) setNavOpen(group, true);
            }
        });
    }

    renderNav();
    window.addEventListener("hashchange", () => {
        spiedId = null;
        updateActiveSidebarItem();
    });

    if (layoutSelect) {
        layoutSelect.value = savedLayout;
        layoutSelect.addEventListener("change", (e) => {
            const layout = e.target.value;
            root.setAttribute("data-layout", layout);
            store("layout", layout);
            renderNav();
        });
    }

    // --- Search: a results panel anchored under the search box. The page
    // underneath stays put, so the reader keeps their place.
    const searchInput = document.getElementById("search-input");
    const searchResults = document.getElementById("search-results");
    const resultsList = document.getElementById("results-list");

    if (searchInput && searchResults && resultsList) {
        // Filled once the index has loaded; until then a query waits for it.
        let entries = null;
        let loadFailed = false;
        let loading = false;
        let activeIndex = -1;

        function ensureIndex() {
            if (entries || loadFailed || loading) return;
            loading = true;
            // Only a panel still showing "Loading search…" is refreshed, so a
            // search the reader already dismissed does not pop back up.
            const waiting = () => searchResults.classList.contains("is-open");
            loadSearchIndex(rootPath).then(index => {
                loading = false;
                entries = collectSearchEntries(index);
                if (waiting()) runSearch();
            }, () => {
                loading = false;
                loadFailed = true;
                if (waiting()) runSearch();
                loadFailed = false;
            });
        }

        const items = () => Array.from(resultsList.querySelectorAll(".search-item"));
        // Loading, "no results" and "N more" messages. Templates from before
        // there was a status line get them inside the results list instead.
        const searchStatus = document.getElementById("search-status");

        // The input is a combobox that owns the results listbox: focus stays
        // in the input and the active option is announced through
        // aria-activedescendant.
        searchInput.setAttribute("role", "combobox");
        searchInput.setAttribute("aria-autocomplete", "list");
        searchInput.setAttribute("aria-controls", resultsList.id);
        searchInput.setAttribute("aria-expanded", "false");

        function showStatus(html) {
            if (searchStatus) {
                searchStatus.innerHTML = html;
            } else if (html) {
                resultsList.insertAdjacentHTML("beforeend", html);
            }
        }

        function openPanel() {
            positionPanel();
            searchResults.classList.add("is-open");
            searchInput.setAttribute("aria-expanded", "true");
        }

        function positionPanel() {
            const box = searchInput.getBoundingClientRect();
            const margin = 12;
            const width = Math.min(Math.max(box.width, 460), window.innerWidth - margin * 2);
            const left = Math.min(Math.max(box.left, margin), window.innerWidth - width - margin);
            searchResults.style.left = `${left}px`;
            searchResults.style.top = `${box.bottom + 6}px`;
            searchResults.style.width = `${width}px`;
            searchResults.style.maxHeight = `${Math.max(window.innerHeight - box.bottom - 24, 160)}px`;
        }

        function setActive(index) {
            const all = items();
            activeIndex = all.length ? (index + all.length) % all.length : -1;
            all.forEach((item, i) => {
                const isActive = i === activeIndex;
                item.classList.toggle("is-active", isActive);
                item.setAttribute("aria-selected", isActive ? "true" : "false");
                if (isActive) item.scrollIntoView({ block: "nearest" });
            });
            if (activeIndex >= 0) searchInput.setAttribute("aria-activedescendant", all[activeIndex].id);
            else searchInput.removeAttribute("aria-activedescendant");
        }

        function closeSearch() {
            searchResults.classList.remove("is-open");
            searchInput.setAttribute("aria-expanded", "false");
            searchInput.removeAttribute("aria-activedescendant");
            activeIndex = -1;
        }

        function runSearch() {
            const query = searchInput.value.toLowerCase().trim();
            if (!query) {
                closeSearch();
                return;
            }

            if (!entries) {
                // The index is still on its way; the query runs when it arrives.
                resultsList.innerHTML = "";
                showStatus(loadFailed
                    ? `<span class="search-empty">The search index could not be loaded.</span>`
                    : `<span class="search-empty search-loading">Loading search…</span>`);
                openPanel();
                setActive(-1);
                ensureIndex();
                return;
            }

            const matches = findMatches(entries, query);
            const shown = matches.slice(0, SEARCH_LIMIT);
            resultsList.innerHTML = shown.map((item, i) => `
                <a class="search-item" id="search-option-${i}" role="option" aria-selected="false" tabindex="-1" href="${rootPath}${escapeHtml(item.link)}">
                    <span class="search-item-type">${item.type}</span>
                    <span class="search-item-name">${highlightMatch(item.name, query)}</span>
                    ${item.summary ? `<span class="search-item-summary">${highlightMatch(item.summary, query)}</span>` : ""}
                </a>`).join("");
            if (matches.length === 0) {
                showStatus(`<span class="search-empty">No results for “${escapeHtml(searchInput.value.trim())}”.</span>`);
            } else if (matches.length > shown.length) {
                showStatus(`<span class="search-empty">${matches.length} results, showing ${shown.length}. Keep typing to narrow down.</span>`);
            } else {
                showStatus(`<span class="visually-hidden">${matches.length} result${matches.length === 1 ? "" : "s"}.</span>`);
            }

            openPanel();
            setActive(matches.length ? 0 : -1);
        }

        searchInput.addEventListener("input", runSearch);
        searchInput.addEventListener("focus", () => {
            // Focusing the box (by click, Tab, "/" or Ctrl+K) starts loading
            // the index, so it is usually there before the first key.
            ensureIndex();
            if (searchInput.value.trim()) runSearch();
        });

        searchInput.addEventListener("keydown", (e) => {
            const open = searchResults.classList.contains("is-open");
            if (e.key === "ArrowDown" || e.key === "ArrowUp") {
                e.preventDefault();
                if (!open) {
                    runSearch();
                } else {
                    setActive(activeIndex + (e.key === "ArrowDown" ? 1 : -1));
                }
            } else if (e.key === "Enter") {
                const target = items()[activeIndex];
                if (target && open) {
                    e.preventDefault();
                    closeSearch();
                    window.location.href = target.href;
                }
            } else if (e.key === "Escape") {
                // First Esc closes the results, the next clears the box.
                // Focus stays in the search box either way.
                e.preventDefault();
                if (open) {
                    closeSearch();
                } else {
                    searchInput.value = "";
                }
            }
        });

        // "/" or Ctrl/Cmd+K jumps to search from anywhere on the page.
        document.addEventListener("keydown", (e) => {
            const typing = /^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName) || e.target.isContentEditable;
            const slash = e.key === "/" && !typing && !e.ctrlKey && !e.metaKey && !e.altKey;
            const ctrlK = (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k";
            if (slash || ctrlK) {
                e.preventDefault();
                searchInput.focus();
                searchInput.select();
            }
        });

        document.addEventListener("click", (e) => {
            if (!searchResults.contains(e.target) && e.target !== searchInput) closeSearch();
        });
        resultsList.addEventListener("click", (e) => {
            if (e.target.closest(".search-item")) closeSearch();
        });
        window.addEventListener("resize", () => {
            if (searchResults.classList.contains("is-open")) positionPanel();
        });
    }

    // --- Scroll spy: highlight the item being read. The URL is left alone,
    // so scrolling never rewrites the address bar.
    const trackedItems = document.querySelectorAll(".item-card[id]");
    if (trackedItems.length > 0) {
        const observerOptions = {
            root: null,
            rootMargin: "-10% 0px -70% 0px",
            threshold: 0
        };

        const observer = new IntersectionObserver((entries) => {
            entries.forEach(entry => {
                if (entry.isIntersecting) {
                    spiedId = entry.target.getAttribute("id");
                    updateActiveSidebarItem();
                }
            });
        }, observerOptions);

        trackedItems.forEach(item => observer.observe(item));
    }

    // --- Copy buttons on code blocks ---
    document.querySelectorAll(".main-content pre").forEach(pre => {
        const button = document.createElement("button");
        button.type = "button";
        button.className = "copy-button";
        button.textContent = "Copy";
        button.setAttribute("aria-label", "Copy code to clipboard");
        button.addEventListener("click", () => {
            const code = pre.querySelector("code") || pre;
            const done = (label) => {
                button.textContent = label;
                setTimeout(() => { button.textContent = "Copy"; }, 1500);
            };
            if (navigator.clipboard && navigator.clipboard.writeText) {
                navigator.clipboard.writeText(code.textContent).then(() => done("Copied"), () => done("Failed"));
            } else {
                done("Failed");
            }
        });
        const wrapper = document.createElement("div");
        wrapper.className = "code-block";
        pre.parentNode.insertBefore(wrapper, pre);
        wrapper.appendChild(pre);
        wrapper.appendChild(button);
    });

    // --- Sidebar resizer ---
    const sidebar = document.querySelector('.sidebar');
    const resizer = document.querySelector('.sidebar-resizer');

    // Small screens: the tree and settings collapse behind a Menu button.
    const navToggle = document.getElementById("nav-toggle");
    if (sidebar && navToggle) {
        navToggle.addEventListener("click", () => {
            const open = sidebar.classList.toggle("nav-open");
            navToggle.setAttribute("aria-expanded", open ? "true" : "false");
        });
    }

    if (sidebar && resizer) {
        let isResizing = false;

        const savedWidth = parseInt(stored("sidebar-width"), 10);
        if (savedWidth >= 200 && savedWidth <= 600) {
            sidebar.style.width = `${savedWidth}px`;
        }

        resizer.addEventListener('mousedown', (e) => {
            isResizing = true;
            e.preventDefault();
            document.body.style.cursor = 'col-resize';
            resizer.classList.add('is-resizing');
        });

        document.addEventListener('mousemove', (e) => {
            if (!isResizing) return;
            let newWidth = e.clientX;
            if (newWidth >= 200 && newWidth <= 600) {
                sidebar.style.width = `${newWidth}px`;
            }
        });

        document.addEventListener('mouseup', () => {
            if (isResizing) {
                isResizing = false;
                document.body.style.cursor = 'default';
                resizer.classList.remove('is-resizing');
                store("sidebar-width", String(Math.round(sidebar.getBoundingClientRect().width)));
            }
        });
    }
});

// --version-dir builds: fill the version <select> from ../versions.json and
// open the same page in the chosen version, or that version's index when
// the page does not exist there. Read at runtime, so older builds list
// versions published after them.
function setupVersionSwitcher() {
    const select = document.getElementById("version-switcher");
    const nav = document.getElementById("nav-tree");
    if (!select) return;
    const rootUrl = new URL((nav && nav.dataset.rootPath) || "./", location.href);
    fetch(new URL("../versions.json", rootUrl))
        .then(response => (response.ok ? response.json() : []))
        .then(versions => {
            if (!Array.isArray(versions) || versions.length === 0) return;
            select.replaceChildren(...versions.map(v => {
                const option = document.createElement("option");
                option.value = v.path;
                option.textContent = "v" + v.version + (v.latest ? " (latest)" : "");
                option.selected = v.version === select.dataset.current;
                return option;
            }));
        })
        .catch(() => {});
    select.addEventListener("change", async () => {
        if (!select.value) return;
        const base = new URL("../" + encodeURIComponent(select.value) + "/", rootUrl);
        const here = location.href.split("#")[0];
        const page = here.startsWith(rootUrl.href) ? here.slice(rootUrl.href.length) : "";
        let target = new URL(page || "index.html", base);
        try {
            const response = await fetch(target, { method: "HEAD" });
            if (!response.ok) target = new URL("index.html", base);
        } catch (e) {
            target = new URL("index.html", base);
        }
        location.href = target.href;
    });
}

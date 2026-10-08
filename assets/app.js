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
        return `<div class="nav-group-flat ${isActive ? "active-mod" : ""}" id="nav-group-${slug}">
            <a class="nav-item-link" href="${targetUrl}"><span class="nav-icon">${ICONS.file}</span>${escapeHtml(mod.name)}</a>
        </div>`;
    }

    return `<details class="nav-group" id="nav-group-${slug}" ${isActive ? "open" : ""}>
        <summary class="nav-header ${isActive ? "active-mod" : ""}">
            <a href="${targetUrl}"><span class="nav-icon">${ICONS.file}</span>${escapeHtml(mod.name)}</a>
            <span class="nav-chevron">${ICONS.chevron}</span>
        </summary>
        <ul>${renderSymbolItems(targetUrl, mod.classes, mod.functions, "li", null)}</ul>
    </details>`;
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
        ? `<a href="${folderUrl}"><span class="nav-icon">${ICONS.folder}</span>${label}</a>`
        : `<span><span class="nav-icon">${ICONS.folder}</span>${label}</span>`;

    const symbolsInner = folderUrl
        ? renderSymbolItems(folderUrl, group.classes, group.functions, "div", "nav-group-flat")
        : "";

    const modulesInner = (group.modules || []).map(mod => renderModuleEntry(mod, rootPath, currentPath)).join("");
    const childrenInner = (group.children || []).map(child => renderNavGroup(child, rootPath, currentPath)).join("");

    return `<details class="nav-folder" open>
        <summary class="folder-header ${folderIsActive ? "active-mod" : ""}">${headerInner}<span class="nav-chevron">${ICONS.chevron}</span></summary>
        <div class="folder-contents">${symbolsInner}${modulesInner}${childrenInner}</div>
    </details>`;
}

function renderNavTree(navData, rootPath, currentPath) {
    return nestNavGroups(navData).map(group => renderNavGroup(group, rootPath, currentPath)).join("");
}

// --- Search (names and one-line summaries from search-index.js) ---
const SEARCH_LIMIT = 40;

function collectSearchEntries(index) {
    const entries = [];
    (index.modules || []).forEach(mod => {
        entries.push({ type: "Module", name: mod.name, summary: mod.summary, link: mod.link_path });
        (mod.classes || []).forEach(cls => {
            entries.push({ type: "Class", name: `${mod.name}.${cls.name}`, summary: cls.summary, link: `${mod.link_path}#class.${cls.name}` });
            (cls.functions || []).forEach(f => {
                entries.push({ type: "Method", name: `${mod.name}.${cls.name}.${f.name}`, summary: f.summary, link: `${mod.link_path}#method.${cls.name}.${f.name}` });
            });
        });
        (mod.functions || []).forEach(f => {
            entries.push({ type: "Function", name: `${mod.name}.${f.name}`, summary: f.summary, link: `${mod.link_path}#fn.${f.name}` });
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

    function bindCollapseState() {
        const detailsElements = document.querySelectorAll("details.nav-group");
        detailsElements.forEach(details => {
            const id = details.getAttribute("id");
            const savedState = stored(id);
            if (savedState !== null) {
                if (savedState === "open") details.setAttribute("open", "");
                else details.removeAttribute("open");
            }
            details.addEventListener("toggle", () => {
                store(id, details.open ? "open" : "closed");
            });
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

    // Marks the sidebar link for the symbol in the URL hash. Links are
    // compared as resolved URLs, so the "../" prefix on nested pages is irrelevant.
    function updateActiveSidebarItem() {
        const here = window.location.pathname + decodeURIComponent(window.location.hash);
        document.querySelectorAll(".nav-item-link").forEach(link => {
            const target = link.pathname + decodeURIComponent(link.hash);
            const isMatch = !!link.hash && target === here;
            link.classList.toggle("active-item", isMatch);
            if (isMatch) {
                const parentDetails = link.closest("details");
                if (parentDetails) parentDetails.open = true;
            }
        });
    }

    renderNav();
    window.addEventListener("hashchange", updateActiveSidebarItem);

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

    if (searchInput && searchResults && resultsList && typeof searchIndex !== "undefined") {
        const entries = collectSearchEntries(searchIndex);
        let activeIndex = -1;

        const items = () => Array.from(resultsList.querySelectorAll(".search-item"));

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
        }

        function closeSearch() {
            searchResults.classList.remove("is-open");
            activeIndex = -1;
        }

        function runSearch() {
            const query = searchInput.value.toLowerCase().trim();
            if (!query) {
                closeSearch();
                return;
            }

            const matches = findMatches(entries, query);
            if (matches.length === 0) {
                resultsList.innerHTML = `<p class="search-empty">No results for “${escapeHtml(searchInput.value.trim())}”.</p>`;
            } else {
                const shown = matches.slice(0, SEARCH_LIMIT);
                let html = shown.map(item => `
                    <a class="search-item" role="option" aria-selected="false" href="${rootPath}${escapeHtml(item.link)}">
                        <span class="search-item-type">${item.type}</span>
                        <span class="search-item-name">${highlightMatch(item.name, query)}</span>
                        ${item.summary ? `<span class="search-item-summary">${highlightMatch(item.summary, query)}</span>` : ""}
                    </a>`).join("");
                if (matches.length > shown.length) {
                    html += `<p class="search-empty">${matches.length - shown.length} more — keep typing to narrow down.</p>`;
                }
                resultsList.innerHTML = html;
            }

            positionPanel();
            searchResults.classList.add("is-open");
            setActive(matches.length ? 0 : -1);
        }

        searchInput.addEventListener("input", runSearch);
        searchInput.addEventListener("focus", () => {
            if (searchInput.value.trim()) runSearch();
        });

        searchInput.addEventListener("keydown", (e) => {
            if (e.key === "ArrowDown") {
                e.preventDefault();
                setActive(activeIndex + 1);
            } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setActive(activeIndex - 1);
            } else if (e.key === "Enter") {
                const target = items()[activeIndex];
                if (target) {
                    e.preventDefault();
                    closeSearch();
                    window.location.href = target.href;
                }
            } else if (e.key === "Escape") {
                searchInput.value = "";
                closeSearch();
                searchInput.blur();
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

    // --- Scroll spy: keep the URL hash in step with the item being read ---
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
                    const id = entry.target.getAttribute("id");
                    history.replaceState(null, null, `#${id}`);
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

    if (sidebar && resizer) {
        let isResizing = false;

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
            }
        });
    }
});

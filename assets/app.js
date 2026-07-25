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
            <a class="nav-item-link" href="${targetUrl}"><span class="nav-icon">📄</span>${escapeHtml(mod.name)}</a>
        </div>`;
    }

    return `<details class="nav-group" id="nav-group-${slug}" ${isActive ? "open" : ""}>
        <summary class="nav-header ${isActive ? "active-mod" : ""}">
            <a href="${targetUrl}"><span class="nav-icon">📄</span>${escapeHtml(mod.name)}</a>
        </summary>
        <ul>${renderSymbolItems(targetUrl, mod.classes, mod.functions, "li", null)}</ul>
    </details>`;
}

function renderNavGroup(group, rootPath, currentPath) {
    // Ungrouped bucket (root-level modules with no folder): just a flat run of entries
    if (!group.name) {
        return (group.modules || []).map(mod => renderModuleEntry(mod, rootPath, currentPath)).join("");
    }

    const folderUrl = group.link_path ? (rootPath + group.link_path) : null;
    const folderIsActive = !!folderUrl && !!currentPath && group.link_path === currentPath;

    const headerInner = folderUrl
        ? `<a href="${folderUrl}"><span class="nav-icon">📁</span>${escapeHtml(group.name)}</a>`
        : `<span><span class="nav-icon">📁</span>${escapeHtml(group.name)}</span>`;

    const symbolsInner = folderUrl
        ? renderSymbolItems(folderUrl, group.classes, group.functions, "div", "nav-group-flat")
        : "";

    const modulesInner = (group.modules || []).map(mod => renderModuleEntry(mod, rootPath, currentPath)).join("");

    return `<details class="nav-folder" open>
        <summary class="folder-header ${folderIsActive ? "active-mod" : ""}">${headerInner}</summary>
        <div class="folder-contents">${symbolsInner}${modulesInner}</div>
    </details>`;
}

function renderNavTree(navData, rootPath, currentPath) {
    return (navData || []).map(group => renderNavGroup(group, rootPath, currentPath)).join("");
}

document.addEventListener("DOMContentLoaded", () => {
    // --- Layout (Classic / Minimal / Modern) + color theme (dark/light/slate) ---
    const layoutSelect = document.getElementById("layout-select");
    const themeSelect = document.getElementById("theme-select");
    const colorRow = document.getElementById("color-theme-selector-row");

    // Fall back to whatever the generator baked in as the site's default
    // (set via --layout/--theme at generation time), not a hardcoded value.
    const bakedLayout = document.documentElement.getAttribute("data-layout") || "classic";
    const bakedTheme = document.documentElement.getAttribute("data-theme") || "dark";

    const savedLayout = localStorage.getItem("layout") || bakedLayout;
    const savedTheme = localStorage.getItem("theme") || bakedTheme;

    function applyColorRowVisibility(layout) {
        if (colorRow) {
            colorRow.classList.toggle("is-hidden", layout !== "classic");
        }
    }

    document.documentElement.setAttribute("data-layout", savedLayout);
    document.documentElement.setAttribute("data-theme", savedTheme);
    applyColorRowVisibility(savedLayout);

    if (layoutSelect) {
        layoutSelect.value = savedLayout;
        layoutSelect.addEventListener("change", (e) => {
            const layout = e.target.value;
            document.documentElement.setAttribute("data-layout", layout);
            localStorage.setItem("layout", layout);
            applyColorRowVisibility(layout);
        });
    }

    if (themeSelect) {
        themeSelect.value = savedTheme;
        themeSelect.addEventListener("change", (e) => {
            const theme = e.target.value;
            document.documentElement.setAttribute("data-theme", theme);
            localStorage.setItem("theme", theme);
        });
    }

    // --- Nav tree: render once from the shared nav-data.js payload ---
    const navContainer = document.getElementById("nav-tree");
    if (navContainer && typeof navData !== "undefined") {
        const rootPath = navContainer.dataset.rootPath || "./";
        const currentPath = navContainer.dataset.currentPath || "";
        navContainer.innerHTML = renderNavTree(navData, rootPath, currentPath);
    }

    // --- Restore collapse-state for per-module dropdowns (folders always start open) ---
    const detailsElements = document.querySelectorAll("details.nav-group");
    detailsElements.forEach(details => {
        const id = details.getAttribute("id");
        const savedState = localStorage.getItem(id);
        if (savedState !== null) {
            if (savedState === "open") details.setAttribute("open", "");
            else details.removeAttribute("open");
        }
        details.addEventListener("toggle", () => {
            localStorage.setItem(id, details.open ? "open" : "closed");
        });
    });

    const searchInput = document.getElementById("search-input");
    const searchResults = document.getElementById("search-results");
    const resultsList = document.getElementById("results-list");
    const pageContent = document.getElementById("page-content");

    if (searchInput && typeof searchIndex !== 'undefined') {
        searchInput.addEventListener("input", (e) => {
            const query = e.target.value.toLowerCase().trim();
            if (!query) {
                searchResults.style.display = "none";
                pageContent.style.display = "block";
                return;
            }

            resultsList.innerHTML = "";
            const matches = [];

            searchIndex.modules.forEach(mod => {
                if (mod.name.toLowerCase().includes(query)) {
                    matches.push({ type: "Module", name: mod.name, link: mod.link_path });
                }
                if (mod.classes) {
                    mod.classes.forEach(cls => {
                        if (cls.name.toLowerCase().includes(query)) {
                            matches.push({ type: "Class", name: `${mod.name}.${cls.name}`, link: `${mod.link_path}#class.${cls.name}` });
                        }
                        if (cls.functions) {
                            cls.functions.forEach(f => {
                                if (f.name.toLowerCase().includes(query)) {
                                    matches.push({ type: "Method", name: `${mod.name}.${cls.name}.${f.name}`, link: `${mod.link_path}#class.${cls.name}` });
                                }
                            });
                        }
                    });
                }
                if (mod.functions) {
                    mod.functions.forEach(f => {
                        if (f.name.toLowerCase().includes(query)) {
                            matches.push({ type: "Function", name: `${mod.name}.${f.name}`, link: `${mod.link_path}#fn.${f.name}` });
                        }
                    });
                }
            });

            if (matches.length > 0) {
                matches.forEach(item => {
                    const itemDiv = document.createElement("div");
                    itemDiv.className = "search-item";
                    itemDiv.innerHTML = `<small style="color: var(--muted-color); text-transform: uppercase; font-size: 0.75rem;">[${item.type}]</small><br><a href="${item.link}">${item.name}</a>`;
                    resultsList.appendChild(itemDiv);
                });
            } else {
                resultsList.innerHTML = "<p>No matching symbols found.</p>";
            }

            pageContent.style.display = "none";
            searchResults.style.display = "block";
        });
    }

    function updateActiveSidebarItem() {
        const currentPath = window.location.pathname.split("/").pop() || "index.html";
        const currentHash = window.location.hash;
        const sidebarLinks = document.querySelectorAll(".nav-item-link");

        sidebarLinks.forEach(link => {
            link.classList.remove("active-item");
            const href = link.getAttribute("href");

            const isMatch = currentHash
                ? href === `${currentPath}${currentHash}`
                : href === currentPath;

            if (isMatch) {
                link.classList.add("active-item");

                const parentDetails = link.closest("details");
                if (parentDetails) {
                    parentDetails.open = true;
                }
            }
        });
    }

    updateActiveSidebarItem();
    window.addEventListener("hashchange", updateActiveSidebarItem);

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
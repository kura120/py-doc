document.addEventListener("DOMContentLoaded", () => {
    // --- 1. Theme Manager ---
    const select = document.getElementById("theme-select");
    const savedTheme = localStorage.getItem("theme") || "dark";
    document.documentElement.setAttribute("data-theme", savedTheme);
    if (select) {
        select.value = savedTheme;
        select.addEventListener("change", (e) => {
            const theme = e.target.value;
            document.documentElement.setAttribute("data-theme", theme);
            localStorage.setItem("theme", theme);
        });
    }

    // --- 2. Keep Collapsed State Memory ---
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

    // --- 3. Live Client-Side Search Engine ---
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
                    matches.push({ type: "Module", name: mod.name, link: `${mod.name}.html` });
                }
                if (mod.classes) {
                    mod.classes.forEach(cls => {
                        if (cls.name.toLowerCase().includes(query)) {
                            matches.push({ type: "Class", name: `${mod.name}.${cls.name}`, link: `${mod.name}.html#class.${cls.name}` });
                        }
                        if (cls.functions) {
                            cls.functions.forEach(f => {
                                if (f.name.toLowerCase().includes(query)) {
                                    matches.push({ type: "Method", name: `${mod.name}.${cls.name}.${f.name}`, link: `${mod.name}.html#class.${cls.name}` });
                                }
                            });
                        }
                    });
                }
                if (mod.functions) {
                    mod.functions.forEach(f => {
                        if (f.name.toLowerCase().includes(query)) {
                            matches.push({ type: "Function", name: `${mod.name}.${f.name}`, link: `${mod.name}.html#fn.${f.name}` });
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

            // Match exact path + hash (or just path if there's no hash)
            const isMatch = currentHash 
                ? href === `${currentPath}${currentHash}`
                : href === currentPath;

            if (isMatch) {
                link.classList.add("active-item");
                
                // Auto-expand the parent <details> folder if it's closed
                const parentDetails = link.closest("details");
                if (parentDetails) {
                    parentDetails.open = true;
                }
            }
        });
    }

    // Run immediately on load and whenever the hash changes
    updateActiveSidebarItem();
    window.addEventListener("hashchange", updateActiveSidebarItem);

    const trackedItems = document.querySelectorAll(".item-card[id]");
    if (trackedItems.length > 0) {
        const observerOptions = {
            root: null,
            rootMargin: "-10% 0px -70% 0px", // Focus on items near the top third of viewport
            threshold: 0
        };

        const observer = new IntersectionObserver((entries) => {
            entries.forEach(entry => {
                if (entry.isIntersecting) {
                    const id = entry.target.getAttribute("id");
                    const currentPath = window.location.pathname.split("/").pop() || "index.html";
                    
                    // Temporarily update hash without triggering scroll jumps
                    history.replaceState(null, null, `#${id}`);
                    
                    // Update the sidebar items to match the new hash
                    updateActiveSidebarItem();
                }
            });
        }, observerOptions);

        trackedItems.forEach(item => observer.observe(item));
    }
});
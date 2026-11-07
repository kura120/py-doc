//! `swiftdoc serve`: builds the site, serves `--out` on 127.0.0.1 and
//! rebuilds (incrementally, through the build cache) whenever the sources,
//! the `--templates` folder or a `--css` file change, then reloads every
//! open page.
//!
//! Live reload is a small script added to HTML responses as they are
//! served; the files on disk are never changed, so a served site is the
//! same as a built one. The script long-polls [`RELOAD_PATH`], which
//! answers with the current build number as soon as it differs from the
//! one the page was loaded with.

use anyhow::{Context, Result, bail};
use notify::{EventKind, RecursiveMode, Watcher};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tiny_http::{Header, Request, Response, Server};

use crate::OutputFormat;
use crate::config::Config;

/// The long-polling endpoint of the reload script.
pub const RELOAD_PATH: &str = "/__swiftdoc/reload";

/// How long a reload poll waits for a new build before answering anyway.
const POLL_WAIT: Duration = Duration::from_secs(25);

/// How long the watcher waits for a burst of changes (an editor's
/// save-and-rename, a `git checkout`) to settle before rebuilding.
const DEBOUNCE: Duration = Duration::from_millis(150);

/// Ports tried after the one asked for when it is taken.
const PORT_ATTEMPTS: u16 = 10;

/// Added before `</body>` of every served HTML page. It asks for the build
/// number, then waits on the server for it to change and reloads the page.
/// When the server is gone it retries every second.
const RELOAD_SCRIPT: &str = r#"<script data-swiftdoc-reload>(function(){var b=null;function p(){fetch("/__swiftdoc/reload"+(b===null?"":"?since="+b),{cache:"no-store"}).then(function(r){if(!r.ok)throw r;return r.text()}).then(function(t){if(b!==null&&t!==b){location.reload();return}b=t;p()}).catch(function(){setTimeout(p,1000)})}p()})();</script>"#;

/// The number of the last build, which pages wait on to change.
#[derive(Default)]
struct Builds {
    number: Mutex<u64>,
    changed: Condvar,
}

impl Builds {
    fn current(&self) -> u64 {
        *self.number.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn bump(&self) {
        *self.number.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        self.changed.notify_all();
    }

    /// The build number once it is not `since`, or after `timeout`.
    fn wait_past(&self, since: u64, timeout: Duration) -> u64 {
        let guard = self.number.lock().unwrap_or_else(|e| e.into_inner());
        let (guard, _) = self
            .changed
            .wait_timeout_while(guard, timeout, |number| *number == since)
            .unwrap_or_else(|e| e.into_inner());
        *guard
    }
}

enum Message {
    Changed(Vec<PathBuf>),
    Stop,
}

/// Runs until Ctrl+C.
pub fn run(config: Config, port: u16, open: bool) -> Result<()> {
    let Some(out) = config.out.clone() else {
        bail!("serve needs --out (or `out` in [tool.swiftdoc]): the folder to build and serve");
    };
    if !config.formats.contains(&OutputFormat::Html) {
        bail!("serve needs the `html` format");
    }
    fs::create_dir_all(&out).with_context(|| format!("Failed to create {}", out))?;
    let out_dir = dunce::canonicalize(&out).with_context(|| format!("Failed to open {}", out))?;

    let builds = Arc::new(Builds::default());
    if let Err(e) = crate::build(&config, true) {
        eprintln!("\x1b[31;1merror:\x1b[0m {:#}", e);
        eprintln!("  Serving what is in the folder; fix it and save to rebuild.");
    }

    let (server, address) = bind(port)?;
    let url = format!("http://{}/", address);
    let serving_dir = out_dir.clone();
    let serving_builds = Arc::clone(&builds);
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let dir = serving_dir.clone();
            let builds = Arc::clone(&serving_builds);
            std::thread::spawn(move || handle(request, &dir, &builds));
        }
    });

    let (tx, rx) = mpsc::channel();
    let watched = watch_list(&config)?;
    let relevant = watched.clone();
    let changes = tx.clone();
    // Reads are reported too (the build itself opens every source), so only
    // changes count, and never those to the site being written.
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        let Ok(event) = result else { return };
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        let paths: Vec<PathBuf> = event
            .paths
            .into_iter()
            .filter(|path| !path.starts_with(&out_dir))
            .filter(|path| relevant.iter().any(|watch| watch.covers(path)))
            .collect();
        if !paths.is_empty() {
            let _ = changes.send(Message::Changed(paths));
        }
    })
    .context("Failed to start watching for changes")?;
    for watch in &watched {
        let mode = if watch.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        watcher
            .watch(&watch.dir, mode)
            .with_context(|| format!("Failed to watch {}", watch.dir.display()))?;
    }

    // The first Ctrl+C stops after the build in progress; a second one
    // stops at once.
    let stopping = AtomicBool::new(false);
    ctrlc::set_handler(move || {
        if stopping.swap(true, Ordering::SeqCst) {
            std::process::exit(130);
        }
        let _ = tx.send(Message::Stop);
    })
    .context("Failed to handle Ctrl+C")?;

    println!(
        "\n\x1b[32;1m✔ Serving {} at {}\x1b[0m",
        out.trim_end_matches(['/', '\\']),
        url
    );
    println!("  Watching for changes. Press Ctrl+C to stop.");
    if open {
        open_browser(&url);
    }

    while let Ok(message) = rx.recv() {
        let mut changed = match message {
            Message::Stop => break,
            Message::Changed(paths) => paths,
        };
        // Wait for the burst to settle: rebuild once nothing has changed
        // for DEBOUNCE.
        let mut stop = false;
        while let Ok(more) = rx.recv_timeout(DEBOUNCE) {
            match more {
                Message::Stop => {
                    stop = true;
                    break;
                }
                Message::Changed(paths) => changed.extend(paths),
            }
        }
        if stop {
            break;
        }
        changed.sort();
        changed.dedup();
        let more = match changed.len() {
            1 => String::new(),
            n => format!(" and {} more", n - 1),
        };
        println!(
            "\n\x1b[36;1m↻\x1b[0m {} changed{}",
            shown(&changed[0]),
            more
        );
        rebuild(&config, &builds);
    }
    println!("\nStopped.");
    Ok(())
}

/// Builds the site; on success, or on a failure after files were written,
/// open pages reload. A failure is printed and the last good site is kept.
fn rebuild(config: &Config, builds: &Builds) {
    let start = Instant::now();
    match crate::build(config, true) {
        Ok(()) => {
            builds.bump();
            println!(
                "  \x1b[32m✔\x1b[0m Rebuilt in {:.2?}; reloading open pages",
                start.elapsed()
            );
        }
        Err(e) => {
            if e.downcast_ref::<crate::NotRebuilt>().is_none() {
                builds.bump();
            }
            eprintln!("\x1b[31;1merror:\x1b[0m {:#}", e);
            eprintln!("  Still serving the last good build; fix it and save again.");
        }
    }
}

/// Binds 127.0.0.1 on `port`, or the next free one.
fn bind(port: u16) -> Result<(Server, std::net::SocketAddr)> {
    let attempts = if port == 0 { 1 } else { PORT_ATTEMPTS };
    let mut last_error = None;
    for offset in 0..attempts {
        let Some(candidate) = port.checked_add(offset) else {
            break;
        };
        match Server::http(("127.0.0.1", candidate)) {
            Ok(server) => {
                let address = server
                    .server_addr()
                    .to_ip()
                    .context("the server is not on a TCP port")?;
                if offset > 0 {
                    println!("  Port {} is in use; using {}.", port, address.port());
                }
                return Ok((server, address));
            }
            Err(e) => last_error = Some(e),
        }
    }
    bail!(
        "could not listen on 127.0.0.1 ports {}-{}: {}",
        port,
        port.saturating_add(attempts - 1),
        last_error.map(|e| e.to_string()).unwrap_or_default()
    )
}

/// A folder to watch, and which paths under it matter.
#[derive(Clone)]
struct Watch {
    dir: PathBuf,
    recursive: bool,
    /// Only these files in `dir` (for `--css` files and a single `--src`
    /// file, whose folders are watched so a save-by-rename is seen).
    files: Vec<PathBuf>,
}

impl Watch {
    fn covers(&self, path: &Path) -> bool {
        if !self.files.is_empty() {
            return self.files.iter().any(|file| file == path);
        }
        let Ok(relative) = path.strip_prefix(&self.dir) else {
            return false;
        };
        // Hidden folders and files (.git, editor swap files) and bytecode
        // caches never change the docs.
        !relative.components().any(|component| match component {
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                name.starts_with('.') || name == "__pycache__" || name.ends_with('~')
            }
            _ => false,
        })
    }
}

fn watch_list(config: &Config) -> Result<Vec<Watch>> {
    let canonical =
        |path: &str| dunce::canonicalize(path).with_context(|| format!("Failed to open {}", path));
    let mut list = Vec::new();
    let src = canonical(&config.src)?;
    if src.is_dir() {
        list.push(Watch {
            dir: src,
            recursive: true,
            files: Vec::new(),
        });
    } else {
        list.push(single_file(src));
    }
    if let Some(templates) = &config.templates {
        list.push(Watch {
            dir: canonical(templates)?,
            recursive: true,
            files: Vec::new(),
        });
    }
    for css in &config.css {
        list.push(single_file(canonical(css)?));
    }
    Ok(list)
}

fn single_file(file: PathBuf) -> Watch {
    let dir = file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Watch {
        dir,
        recursive: false,
        files: vec![file],
    }
}

fn shown(path: &Path) -> String {
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|dir| dunce::canonicalize(dir).ok());
    cwd.and_then(|dir| path.strip_prefix(dir).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}

fn open_browser(url: &str) {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", ""]);
        command
    } else {
        std::process::Command::new("xdg-open")
    };
    if command.arg(url).spawn().is_err() {
        eprintln!("  Could not open a browser; open {} yourself.", url);
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("a valid header")
}

fn handle(request: Request, out_dir: &Path, builds: &Builds) {
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    let no_store = header("Cache-Control", "no-store");

    if path == RELOAD_PATH {
        let since = query
            .split('&')
            .find_map(|pair| pair.strip_prefix("since="))
            .and_then(|value| value.parse::<u64>().ok());
        let number = match since {
            Some(since) => builds.wait_past(since, POLL_WAIT),
            None => builds.current(),
        };
        let response = Response::from_string(number.to_string())
            .with_header(header("Content-Type", "text/plain; charset=utf-8"))
            .with_header(no_store);
        let _ = request.respond(response);
        return;
    }

    let response = match resolve(out_dir, path).and_then(|file| Some((fs::read(&file).ok()?, file)))
    {
        Some((body, file)) => {
            let content_type = content_type(&file);
            let body = if content_type.starts_with("text/html") {
                inject_reload(body)
            } else {
                body
            };
            Response::from_data(body).with_header(header("Content-Type", content_type))
        }
        None => Response::from_string("Not found").with_status_code(404),
    };
    let _ = request.respond(response.with_header(no_store));
}

/// The file under `out_dir` that a request path names, if any. A folder
/// means its index.html; `..` and other escapes are refused.
fn resolve(out_dir: &Path, path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(path)?;
    let mut file = out_dir.to_path_buf();
    for segment in decoded.split('/') {
        match segment {
            "" | "." => {}
            ".." => return None,
            _ if segment.contains('\\') || segment.contains(':') => return None,
            _ => file.push(segment),
        }
    }
    if file.is_dir() {
        file.push("index.html");
    }
    file.is_file().then_some(file)
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// `body` with the reload script before its last `</body>`, or at the end.
fn inject_reload(mut body: Vec<u8>) -> Vec<u8> {
    let lower = body.to_ascii_lowercase();
    let at = lower
        .windows(b"</body>".len())
        .rposition(|window| window == b"</body>")
        .unwrap_or(body.len());
    body.splice(at..at, RELOAD_SCRIPT.bytes());
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_goes_before_the_body_end() {
        let page = inject_reload(b"<html><body><p>x</p></BODY></html>".to_vec());
        let page = String::from_utf8(page).unwrap();
        assert!(page.starts_with("<html><body><p>x</p><script data-swiftdoc-reload>"));
        assert!(page.ends_with("</script></BODY></html>"));
        let bare = String::from_utf8(inject_reload(b"<p>x</p>".to_vec())).unwrap();
        assert!(bare.starts_with("<p>x</p><script"));
    }

    #[test]
    fn paths_stay_inside_the_site() {
        let dir = std::env::temp_dir().join(format!("swiftdoc-serve-{}", std::process::id()));
        fs::create_dir_all(dir.join("pkg")).unwrap();
        fs::write(dir.join("index.html"), "i").unwrap();
        fs::write(dir.join("pkg").join("a b.html"), "a").unwrap();
        assert_eq!(resolve(&dir, "/"), Some(dir.join("index.html")));
        assert_eq!(
            resolve(&dir, "/pkg/a%20b.html"),
            Some(dir.join("pkg").join("a b.html"))
        );
        assert_eq!(resolve(&dir, "/../index.html"), None);
        assert_eq!(resolve(&dir, "/pkg/%2e%2e/%2e%2e/etc/passwd"), None);
        assert_eq!(resolve(&dir, "/missing.html"), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn hidden_and_cache_paths_do_not_trigger_rebuilds() {
        let watch = Watch {
            dir: PathBuf::from("/p"),
            recursive: true,
            files: Vec::new(),
        };
        assert!(watch.covers(Path::new("/p/pkg/a.py")));
        assert!(!watch.covers(Path::new("/p/.git/index")));
        assert!(!watch.covers(Path::new("/p/pkg/__pycache__/a.pyc")));
        assert!(!watch.covers(Path::new("/elsewhere/a.py")));
        let file = single_file(PathBuf::from("/p/extra.css"));
        assert!(file.covers(Path::new("/p/extra.css")));
        assert!(!file.covers(Path::new("/p/other.css")));
    }
}

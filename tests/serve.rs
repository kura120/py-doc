//! `swiftdoc serve`: the live-reload script is only in served pages, edits
//! rebuild and reach the browser, a broken file keeps the last good site,
//! and Ctrl+C stops cleanly. Every wait polls with a deadline.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(15);

/// The server process; killed if a test fails before stopping it.
struct Served {
    child: Child,
    port: u16,
    /// Every line the server prints, stdout and stderr.
    lines: mpsc::Receiver<String>,
    seen: Vec<String>,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Served {
    fn start(src: &Path, out: &Path, config: &Path) -> Served {
        let mut child = Command::new(env!("CARGO_BIN_EXE_swiftdoc"))
            .arg("serve")
            .arg("--config")
            .arg(config)
            .arg("--src")
            .arg(src)
            .arg("--out")
            .arg(out)
            .args(["--name", "demo", "--port", "0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to start swiftdoc serve");
        let (tx, lines) = mpsc::channel();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let out_tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = out_tx.send(line);
            }
        });
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
        let mut served = Served {
            child,
            port: 0,
            lines,
            seen: Vec::new(),
        };
        let line = served.wait_for_line("Serving");
        let port = line
            .split("http://127.0.0.1:")
            .nth(1)
            .and_then(|rest| rest.split('/').next())
            .and_then(|port| port.parse().ok())
            .unwrap_or_else(|| panic!("no URL in {line:?}"));
        served.port = port;
        served
    }

    /// The first line from now on that contains `text`.
    fn wait_for_line(&mut self, text: &str) -> String {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(line) => {
                    self.seen.push(line.clone());
                    if line.contains(text) {
                        return line;
                    }
                }
                Err(_) => panic!("no line with {text:?}; output:\n{}", self.seen.join("\n")),
            }
        }
    }

    /// Status code and body of a GET.
    fn get(&self, path: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        let response = String::from_utf8_lossy(&response).into_owned();
        let (head, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
        let status = head
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or(0);
        (status, body.to_string())
    }

    /// Polls `path` until its body contains `text`.
    fn wait_for_page(&self, path: &str, text: &str) -> String {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let (status, body) = self.get(path);
            if status == 200 && body.contains(text) {
                return body;
            }
            assert!(
                Instant::now() < deadline,
                "{path} never contained {text:?}; last body:\n{body}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

fn setup(test: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("serve")
        .join(test);
    let _ = fs::remove_dir_all(&root);
    let src = root.join("src");
    fs::create_dir_all(src.join("pkg")).unwrap();
    fs::write(
        src.join("pkg/__init__.py"),
        "\"\"\"The demo package.\"\"\"\n",
    )
    .unwrap();
    fs::write(
        src.join("pkg/engine.py"),
        "\"\"\"Engine, first version.\"\"\"\n\n\ndef start():\n    \"\"\"Starts.\"\"\"\n",
    )
    .unwrap();
    let config = root.join("empty-pyproject.toml");
    fs::write(&config, "").unwrap();
    (src, root.join("site"), config)
}

#[test]
fn serve_injects_reload_rebuilds_and_survives_errors() {
    let (src, out, config) = setup("live");
    let mut served = Served::start(&src, &out, &config);

    // The served page has the reload script; the file on disk does not,
    // and is otherwise the same page.
    let page = served.wait_for_page("/pkg/engine.html", "Engine, first version.");
    assert!(page.contains("data-swiftdoc-reload"), "{page}");
    let on_disk = fs::read_to_string(out.join("pkg/engine.html")).unwrap();
    assert!(!on_disk.contains("swiftdoc-reload"));
    assert!(!on_disk.contains("__swiftdoc"));
    let script_start = page.find("<script data-swiftdoc-reload>").unwrap();
    let script_end = page[script_start..].find("</script>").unwrap() + "</script>".len();
    let without = format!(
        "{}{}",
        &page[..script_start],
        &page[script_start + script_end..]
    );
    assert_eq!(without, on_disk);
    for file in walk(&out) {
        let bytes = fs::read(&file).unwrap();
        assert!(
            !String::from_utf8_lossy(&bytes).contains("__swiftdoc/reload"),
            "{} has the reload code",
            file.display()
        );
    }
    // A folder serves its index; nothing outside the site is reachable.
    assert!(served.get("/").1.contains("data-swiftdoc-reload"));
    assert_eq!(served.get("/../empty-pyproject.toml").0, 404);
    assert_eq!(served.get("/missing.html").0, 404);

    let (_, build) = served.get("/__swiftdoc/reload");

    // An edit rebuilds the page, and a waiting reload poll answers.
    let engine = src.join("pkg/engine.py");
    let text = fs::read_to_string(&engine).unwrap();
    fs::write(&engine, text.replace("first", "second")).unwrap();
    served.wait_for_page("/pkg/engine.html", "Engine, second version.");
    let (status, newer) = served.get(&format!("/__swiftdoc/reload?since={build}"));
    assert_eq!(status, 200);
    assert_ne!(newer, build);

    // A syntax error is reported and the last good site stays served.
    fs::write(src.join("pkg/broken.py"), "def broken(:\n").unwrap();
    served.wait_for_line("the site was not rebuilt");
    let (status, page) = served.get("/pkg/engine.html");
    assert_eq!(status, 200);
    assert!(page.contains("Engine, second version."));
    assert_eq!(served.get("/__swiftdoc/reload").1, newer);

    // The next good save recovers.
    fs::remove_file(src.join("pkg/broken.py")).unwrap();
    fs::write(&engine, text.replace("first", "third")).unwrap();
    served.wait_for_page("/pkg/engine.html", "Engine, third version.");

    // Ctrl+C stops it cleanly.
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .args(["-INT", &served.child.id().to_string()])
            .status()
            .unwrap();
        assert!(status.success());
        served.wait_for_line("Stopped.");
        let deadline = Instant::now() + DEADLINE;
        let exit = loop {
            if let Some(exit) = served.child.try_wait().unwrap() {
                break exit;
            }
            assert!(Instant::now() < deadline, "serve did not exit");
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(exit.success(), "{exit:?}");
    }
}

#[test]
fn serve_moves_to_the_next_port_when_one_is_taken() {
    let (src, out, config) = setup("port");
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_swiftdoc"))
        .arg("serve")
        .arg("--config")
        .arg(&config)
        .arg("--src")
        .arg(&src)
        .arg("--out")
        .arg(&out)
        .args(["--name", "demo", "--port", &port.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines().map_while(Result::ok);
    let serving = lines
        .find(|line| line.contains("Serving"))
        .expect("serve printed its URL");
    let _ = child.kill();
    let _ = child.wait();
    drop(taken);
    assert!(!serving.contains(&format!(":{port}/")), "{serving}");
    assert!(serving.contains("http://127.0.0.1:"), "{serving}");
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}

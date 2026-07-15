# py-doc

A lightweight and fast documentation tool for Python codebases, powered by Rust.

## Table of Contents

* [Installation](#installation)
* [Option A: One-Line Script Installer (Easiest)](#option-a-one-line-script-installer-easiest)
* [Option B: Fast Binary Installer (cargo-binstall)](#option-b-fast-binary-installer-cargo-binstall)
* [Option C: Build from Source](#option-c-build-from-source)


* [Usage](#usage)
* [Troubleshooting](#troubleshooting)

---

## Installation

Choose the installation method that fits your environment.

### Option A: One-Line Script Installer (Easiest)

You do not need Cargo or Rust installed for this. Run the installer script for your OS in your terminal. This downloads the pre-compiled binary, installs it, and configures your path.

* **macOS / Linux (Bash/Zsh):**
```bash
curl -fsSL https://raw.githubusercontent.com/kura120/py-doc/main/install.sh | sh

```


* **Windows (PowerShell - Run as Administrator):**
```powershell
irm https://raw.githubusercontent.com/yourusername/py-doc/main/install.ps1 | iex

```



---

### Option B: Fast Binary Installer (cargo-binstall)

If you have Cargo but want to avoid long compile times and dependency conflicts, install `py-doc` as a pre-compiled binary directly via `cargo-binstall`.

```bash
# Install cargo-binstall if you don't have it
cargo install cargo-binstall

# Install py-doc quickly
cargo binstall py-doc

```

---

### Option C: Build from Source

If you want to compile the codebase locally from source:

```bash
# Clone the repository
git clone https://github.com/kura120/py-doc.git
cd py-doc

# Install local binary
cargo install --path .

```

---

## Usage

### Interactive CLI Help

You can access details on commands, arguments, and flags directly from the CLI:

```bash
py-doc --help

```

### Basic Commands

Run `py-doc` against any Python file or directory:

```bash
py-doc --src path/to/project --out ./docs --name project-name
```

---

## Troubleshooting

### 'py-doc' is not recognized as a command

If you chose Option C (or your script path environment variable is missing), your shell cannot locate the executable. You must add Cargo's binary directory to your PATH.

* **Windows (PowerShell):**
```powershell
[Environment]::SetEnvironmentVariable("Path", "$env:Path;$env:USERPROFILE\.cargo\bin", "User")

```


* **macOS / Linux:**
```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc

```



### Dependency Conflicts during Local Compilation

If you see a compiler error stating `the trait bound 'CompactString: GetSize' is not satisfied` during source installations, run:

```powershell
cargo clean
cargo install --path .

```

This forces Cargo to respect the exact version constraint specified in the manifest.
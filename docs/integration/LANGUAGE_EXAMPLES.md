# Deltaship Integration — Language-Specific Examples

Concrete, copy-pasteable examples for embedding the `deltaship-updater` **sidecar** into
applications written in different languages, for every supported platform. Read
[INTEGRATION_GUIDE.md](./INTEGRATION_GUIDE.md) first for the architecture and the
exit-code contract.

**The whole integration is the same everywhere:**

1. Bundle the per-platform `deltaship-updater` binary and your `public.key` next to your app.
2. On launch, spawn the updater with four required args.
3. Read the exit code: **`0`** = up to date, **`2`** = updated → relaunch your app,
   **`1`** = error (non-fatal; keep running on the current version).

```
deltaship-updater \
  --name        <app-id> \
  --install-path <path to the binary to update> \
  --server-url  https://updates.example.com \
  --public-key  <path to bundled public.key> \
  [--data-dir <state dir>] [--check-only] [-q]
```

Exit codes are the contract — branch on them, don't parse stdout.

- [Platform matrix](#platform-matrix)
- [Shell / cross-platform launcher](#shell)
- [C](#c)
- [C++](#c-1)
- [C# / .NET](#c--net)
- [Go](#go)
- [Python](#python)
- [Node.js / Electron](#nodejs--electron)
- [Java](#java)
- [Rust (sidecar)](#rust-sidecar)
- [Rust (in-process library)](#rust-in-process-library)
- [Windows launcher pattern](#windows-launcher-pattern)

---

## Platform matrix

Ship the matching updater binary per target. The updater auto-detects its platform
id at runtime, so you pass the **same args** everywhere — only the binary filename
and install path differ.

| Platform id | Updater file you bundle | Typical install path you pass |
|---|---|---|
| `linux-x86_64` / `linux-aarch64` | `deltaship-updater` | `/opt/myapp/bin/myapp` or `~/.local/bin/myapp` |
| `windows-x86_64` | `deltaship-updater.exe` | `%LOCALAPPDATA%\MyApp\myapp.exe` |
| `macos-x86_64` / `macos-aarch64` | `deltaship-updater` | `MyApp.app/Contents/MacOS/myapp` |

> Never install under `C:\Windows`, `C:\Program Files[ (x86)]`, `/usr`, `/etc`,
> etc. — the updater refuses privileged locations.

---

## Shell

A portable launcher wrapper (Linux/macOS). Run the updater, relaunch on `2`.

```sh
#!/bin/sh
# myapp-launch.sh — run updater, then start the app.
APPDIR="$(cd "$(dirname "$0")" && pwd)"

"$APPDIR/deltaship-updater" \
  --name        myapp \
  --install-path "$APPDIR/myapp" \
  --server-url  https://updates.example.com \
  --public-key  "$APPDIR/publisher.pub"
code=$?

case "$code" in
  0) : ;;                                   # up to date
  2) echo "Updated to a new version." ;;    # updated; we'll exec the fresh binary below
  1) echo "Update check failed; continuing on current version." >&2 ;;
esac

exec "$APPDIR/myapp" "$@"                    # always launch the (possibly new) binary
```

Windows `.cmd` equivalent:

```bat
@echo off
set "APPDIR=%~dp0"
"%APPDIR%deltaship-updater.exe" --name myapp --install-path "%APPDIR%myapp.exe" ^
  --server-url https://updates.example.com --public-key "%APPDIR%publisher.pub"
if %ERRORLEVEL%==1 echo Update check failed; continuing. 1>&2
start "" "%APPDIR%myapp.exe" %*
```

---

## C

```c
#include <stdio.h>
#include <stdlib.h>

#ifdef _WIN32
#  include <process.h>
#  define UPDATER "deltaship-updater.exe"
#else
#  include <sys/wait.h>
#  include <unistd.h>
#  define UPDATER "./deltaship-updater"
#endif

/* Returns updater exit code: 0 up-to-date, 2 updated, 1 error. */
static int run_updater(void) {
    const char *argv[] = {
        UPDATER,
        "--name",         "myapp",
        "--install-path", "/opt/myapp/bin/myapp",
        "--server-url",   "https://updates.example.com",
        "--public-key",   "/opt/myapp/publisher.pub",
        NULL
    };
#ifdef _WIN32
    /* _spawnv returns the child's exit code directly. */
    intptr_t rc = _spawnv(_P_WAIT, UPDATER, argv);
    return (int)rc;
#else
    pid_t pid = fork();
    if (pid == 0) { execv(UPDATER, (char *const *)argv); _exit(1); }
    int status = 0; waitpid(pid, &status, 0);
    return WIFEXITED(status) ? WEXITSTATUS(status) : 1;
#endif
}

int main(void) {
    int code = run_updater();
    if (code == 2) {
        /* New binary written. Re-exec it so the user runs the latest. */
        execl("/opt/myapp/bin/myapp", "myapp", (char *)NULL);
        perror("re-exec failed");
        return 1;
    }
    if (code == 1) fprintf(stderr, "update check failed; continuing\n");
    /* ... start the app normally ... */
    return 0;
}
```

---

## C++

```cpp
#include <cstdlib>
#include <iostream>
#include <string>
#include <vector>

#ifdef _WIN32
#  include <process.h>
#else
#  include <sys/wait.h>
#  include <unistd.h>
#endif

int run_updater() {
    std::vector<const char*> args = {
#ifdef _WIN32
        "deltaship-updater.exe",
#else
        "./deltaship-updater",
#endif
        "--name", "myapp",
        "--install-path", "/opt/myapp/bin/myapp",
        "--server-url", "https://updates.example.com",
        "--public-key", "/opt/myapp/publisher.pub",
        nullptr
    };
#ifdef _WIN32
    return static_cast<int>(_spawnv(_P_WAIT, args[0], args.data()));
#else
    pid_t pid = fork();
    if (pid == 0) { execv(args[0], const_cast<char* const*>(args.data())); _exit(1); }
    int status = 0; waitpid(pid, &status, 0);
    return WIFEXITED(status) ? WEXITSTATUS(status) : 1;
#endif
}

int main() {
    switch (run_updater()) {
        case 0: break;                                   // up to date
        case 2: /* relaunch the new binary */ break;     // updated
        default: std::cerr << "update failed; continuing\n";
    }
    // ... run app ...
}
```

---

## C# / .NET

```csharp
using System;
using System.Diagnostics;

static int RunUpdater()
{
    var exe = OperatingSystem.IsWindows() ? "deltaship-updater.exe" : "deltaship-updater";
    var psi = new ProcessStartInfo
    {
        FileName = exe,
        UseShellExecute = false,
    };
    psi.ArgumentList.Add("--name");         psi.ArgumentList.Add("myapp");
    psi.ArgumentList.Add("--install-path"); psi.ArgumentList.Add(@"C:\Users\me\AppData\Local\MyApp\myapp.exe");
    psi.ArgumentList.Add("--server-url");   psi.ArgumentList.Add("https://updates.example.com");
    psi.ArgumentList.Add("--public-key");   psi.ArgumentList.Add(@"C:\Users\me\AppData\Local\MyApp\publisher.pub");

    using var p = Process.Start(psi)!;
    p.WaitForExit();
    return p.ExitCode;   // 0 up-to-date, 2 updated, 1 error
}

int code = RunUpdater();
if (code == 2)
{
    // On Windows you cannot replace a running exe — use the launcher pattern:
    // this updater process is the launcher; now start the (freshly updated) app.
    Process.Start(@"C:\Users\me\AppData\Local\MyApp\myapp.exe");
}
else if (code == 1)
{
    Console.Error.WriteLine("update check failed; continuing");
}
```

---

## Go

```go
package main

import (
	"errors"
	"log"
	"os"
	"os/exec"
	"runtime"
)

func runUpdater() int {
	bin := "./deltaship-updater"
	if runtime.GOOS == "windows" {
		bin = "deltaship-updater.exe"
	}
	cmd := exec.Command(bin,
		"--name", "myapp",
		"--install-path", "/opt/myapp/bin/myapp",
		"--server-url", "https://updates.example.com",
		"--public-key", "/opt/myapp/publisher.pub",
	)
	cmd.Stdout, cmd.Stderr = os.Stdout, os.Stderr
	err := cmd.Run()
	if err == nil {
		return 0
	}
	var ee *exec.ExitError
	if errors.As(err, &ee) {
		return ee.ExitCode() // 2 = updated, 1 = error
	}
	return 1
}

func main() {
	switch runUpdater() {
	case 0: // up to date
	case 2:
		log.Println("updated — relaunching")
		// syscall.Exec the new binary, or os.StartProcess + os.Exit(0)
	default:
		log.Println("update check failed; continuing")
	}
	// ... start app ...
}
```

---

## Python

```python
import subprocess
import sys
import os

def run_updater() -> int:
    exe = "deltaship-updater.exe" if os.name == "nt" else "./deltaship-updater"
    proc = subprocess.run([
        exe,
        "--name", "myapp",
        "--install-path", "/opt/myapp/bin/myapp",
        "--server-url", "https://updates.example.com",
        "--public-key", "/opt/myapp/publisher.pub",
    ])
    return proc.returncode  # 0 up-to-date, 2 updated, 1 error

code = run_updater()
if code == 2:
    print("Updated — relaunching.")
    os.execv("/opt/myapp/bin/myapp", ["myapp", *sys.argv[1:]])  # replace current process
elif code == 1:
    print("Update check failed; continuing on current version.", file=sys.stderr)
# else: up to date — continue
```

---

## Node.js / Electron

```js
const { spawnSync } = require("node:child_process");
const path = require("node:path");
const process = require("node:process");

function runUpdater() {
  const exe = process.platform === "win32" ? "deltaship-updater.exe" : "deltaship-updater";
  const res = spawnSync(path.join(__dirname, exe), [
    "--name", "myapp",
    "--install-path", path.join(__dirname, process.platform === "win32" ? "myapp.exe" : "myapp"),
    "--server-url", "https://updates.example.com",
    "--public-key", path.join(__dirname, "publisher.pub"),
  ], { stdio: "inherit" });
  return res.status ?? 1; // 0 up-to-date, 2 updated, 1 error
}

// In Electron, run this in the main process BEFORE creating the BrowserWindow.
const code = runUpdater();
if (code === 2) {
  const { app } = require("electron");
  app.relaunch();   // restart so the new binary loads
  app.exit(0);
} else if (code === 1) {
  console.error("update check failed; continuing");
}
```

---

## Java

```java
import java.io.IOException;
import java.util.List;

public final class Updater {
    public static int runUpdater() throws IOException, InterruptedException {
        boolean win = System.getProperty("os.name").toLowerCase().contains("win");
        String exe = win ? "deltaship-updater.exe" : "./deltaship-updater";
        Process p = new ProcessBuilder(List.of(
                exe,
                "--name", "myapp",
                "--install-path", "/opt/myapp/bin/myapp",
                "--server-url", "https://updates.example.com",
                "--public-key", "/opt/myapp/publisher.pub"))
            .inheritIO()
            .start();
        return p.waitFor(); // 0 up-to-date, 2 updated, 1 error
    }

    public static void main(String[] args) throws Exception {
        int code = runUpdater();
        switch (code) {
            case 0 -> { /* up to date */ }
            case 2 -> { /* relaunch the updated app */ }
            default -> System.err.println("update check failed; continuing");
        }
        // ... start app ...
    }
}
```

---

## Rust (sidecar)

```rust
use std::process::Command;

fn run_updater() -> i32 {
    let exe = if cfg!(windows) { "deltaship-updater.exe" } else { "./deltaship-updater" };
    Command::new(exe)
        .args([
            "--name", "myapp",
            "--install-path", "/opt/myapp/bin/myapp",
            "--server-url", "https://updates.example.com",
            "--public-key", "/opt/myapp/publisher.pub",
        ])
        .status()
        .ok()
        .and_then(|s| s.code())
        .unwrap_or(1) // 0 up-to-date, 2 updated, 1 error
}

fn main() {
    match run_updater() {
        0 => { /* up to date */ }
        2 => { /* relaunch the updated binary */ }
        _ => eprintln!("update check failed; continuing"),
    }
    // ... run app ...
}
```

## Rust (in-process library)

If your app is Rust, skip the sidecar and drive updates in-process — see
[INTEGRATION_GUIDE.md §12](./INTEGRATION_GUIDE.md#12-rust-in-process-library-api)
for `UpdateChecker` / `apply_update` / `run_daemon` signatures and a worked example.

---

## Windows launcher pattern

You **cannot overwrite a running `.exe`'s on-disk image** on Windows. The robust
pattern is a tiny launcher that updates *then* starts the real app, so the app
binary is never open while it's being replaced:

```
MyApp/
├─ myapp-launcher.exe   ← shortcut/Start-menu points HERE
├─ myapp.exe            ← the actual app; deltaship-updater's --install-path
├─ deltaship-updater.exe
└─ publisher.pub
```

`myapp-launcher.exe` logic:

```text
1. run deltaship-updater.exe --install-path myapp.exe ...   (myapp.exe is NOT running yet)
2. regardless of exit code (0/2/1), then:
3. CreateProcess("myapp.exe")
4. exit
```

Because the launcher (not `myapp.exe`) is what's running during the update, the
updater can freely replace `myapp.exe`. The same idea applies to long-running
services: have the service wrapper run the updater on (re)start, or schedule the
updater for a maintenance window when the service is stopped.

# bamboo

![Bamboo](/bamboo.png)

A terminal multiplexer TUI written in Rust. Run multiple terminal panes stacked vertically in a single window, with mouse support and TOML configuration.

## Install

```sh
cargo install --path .
```

Or just build:

```sh
cargo build --release
# binary at ./target/release/bamboo
```

## Usage

```sh
bamboo                        # uses default config lookup
bamboo --config my.toml       # explicit config file
bamboo --shoot                # create an isolated git worktree (auto-named)
bamboo --shoot my-feature     # create a worktree named "my-feature"
bamboo -s                     # shorthand for --shoot
```

## Keybindings

| Key | Action |
|-----|--------|
| `Alt+j` / `Alt+l` | Focus next pane |
| `Alt+k` / `Alt+h` | Focus previous pane |
| `Alt+n` | Open new shell pane |
| `Alt+w` | Close focused pane |
| `Alt+c` | Collapse / expand focused pane |
| `Ctrl+↑` | Grow focused pane |
| `Ctrl+↓` | Shrink focused pane |
| `Ctrl+q` | Quit |

**Mouse:** click a pane to focus it; scroll wheel to scroll its content; click `[▾]` on the title bar to collapse/expand; click `[x]` to close.

When more panes exist than fit on screen, a `▲ N more above` or `▼ N more below` indicator appears at the screen edge — click it to page the viewport.

## Configuration

Config is loaded in this order:

1. `--config <path>` CLI flag
2. `.bamboo.toml` in the current directory
3. `~/.config/bamboo/config.toml`
4. `$XDG_CONFIG_HOME/bamboo/config.toml`
5. Built-in default (single interactive shell pane)

### Example

```toml
default_shell = "/bin/zsh"
layout = "Scroll"  # or "Fixed"

[[panes]]
name = "Editor"
command = "nvim"
cwd = "~/projects/myapp"

[[panes]]
name = "Server"
command = "npm run dev"
cwd = "~/projects/myapp"
env = { NODE_ENV = "development" }

[[panes]]
name = "Shell"
```

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `default_shell` | string | Shell binary (default: `$SHELL`) |
| `layout` | `"Scroll"` \| `"Fixed"` | Layout mode (default: `Scroll`) |
| `panes[].name` | string | Pane title |
| `panes[].command` | string? | Command to run (omit for interactive shell) |
| `panes[].cwd` | string? | Working directory (`~` supported) |
| `panes[].env` | table? | Extra environment variables |

**Layout modes:**

- `Scroll` — panes are stacked vertically; each has a configurable weight that controls its share of screen height. When panes overflow the terminal height, a viewport scrolls to keep the focused pane visible.
- `Fixed` — panes fill the available area without per-pane weight adjustments.

### Local override

Drop a `.bamboo.toml` in any project directory to get a project-specific layout when you launch bamboo from there.

## Shoots

A **shoot** is an isolated git worktree spun up automatically when you pass `--shoot` / `-s`. It lets you work on a fresh branch without disturbing your main checkout.

```sh
bamboo --shoot            # auto-names the shoot (e.g. "swift-pine")
bamboo --shoot my-feature # explicit name
bamboo -s                 # shorthand
```

What happens:

1. A new branch `worktree-<name>` is created from HEAD.
2. A worktree is checked out at `<git-root>/.bamboo/worktrees/<name>`.
3. Every pane's working directory is redirected to that path.
4. The active shoot name is shown in the footer badge while bamboo is running.

On exit bamboo checks for changes (uncommitted edits or new commits):

- **No changes** — the worktree and branch are cleaned up silently.
- **Changes present** — you are prompted to keep or remove the shoot.

```
Shoot 'swift-pine' has changes (branch: worktree-swift-pine).
  Path: /your/repo/.bamboo/worktrees/swift-pine
Keep shoot? [K]eep / [r]emove:
```

Shoots require a git repository in (or above) the current directory.

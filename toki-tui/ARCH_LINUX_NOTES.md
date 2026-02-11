# Arch Linux Specific Setup

## Cargo Linker Configuration

If you're on Arch Linux and don't have `clang` installed, the TUI needs a local cargo config.

### Already Set Up For You

The `toki-tui/.cargo/config.toml` file is configured to use `gcc` as the linker (instead of `clang`).

This file is:
- ✅ **Local to toki-tui only** - won't affect the main project
- ✅ **In .gitignore** - won't be committed to git
- ✅ **Safe for your setup** - your colleagues won't be affected

### If You Install clang

If you later install `clang` and `lld`, you can delete this file:

```bash
rm -rf toki-tui/.cargo/
```

Then the TUI will use the same build settings as the main project.

### Original Main Project Config

The main project's `.cargo/config.toml` is **unchanged** and still uses:
```toml
[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "linker=clang", "-C", "link-arg=-fuse-ld=lld"]
```

Your colleagues who have `clang` will continue to build normally.

## Why This Works

Cargo looks for `config.toml` in this order:
1. `.cargo/config.toml` in the current package (toki-tui) ← **TUI uses this**
2. `.cargo/config.toml` in the workspace root ← **Main project uses this**

So each can have different settings without interfering!

## Summary

✅ Main project config: **Restored to original** (uses clang)
✅ TUI config: **Local override** (uses gcc) 
✅ Your colleagues: **Not affected**
✅ The TUI: **Builds successfully**

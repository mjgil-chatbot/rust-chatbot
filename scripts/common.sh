#!/usr/bin/env bash

rust_chatbot_repo_toolchain() {
    local repo_root="$1"
    local toolchain_file="$repo_root/rust-toolchain.toml"

    if [ ! -f "$toolchain_file" ]; then
        return 0
    fi

    sed -n 's/^channel = "\(.*\)"/\1/p' "$toolchain_file" | head -n 1
}

rust_chatbot_repo_cargo() {
    local repo_root="$1"
    shift

    local toolchain
    local cargo_bin
    local toolchain_bin
    local toolchain_lib
    local toolchain_ld_library_path
    toolchain="$(rust_chatbot_repo_toolchain "$repo_root")"

    if [ -n "$toolchain" ] && command -v rustup >/dev/null 2>&1; then
        cargo_bin="$(rustup which --toolchain "$toolchain" cargo 2>/dev/null || true)"
        if [ -n "$cargo_bin" ] && [ -x "$cargo_bin" ]; then
            toolchain_bin="$(dirname "$cargo_bin")"
            toolchain_lib="$(cd "$toolchain_bin/.." && pwd)/lib"
            if [ -n "${LD_LIBRARY_PATH:-}" ]; then
                toolchain_ld_library_path="$toolchain_lib:$LD_LIBRARY_PATH"
            else
                toolchain_ld_library_path="$toolchain_lib"
            fi
            PATH="$toolchain_bin:$PATH" \
            LD_LIBRARY_PATH="$toolchain_ld_library_path" \
            RUSTC="$toolchain_bin/rustc" \
            RUSTDOC="$toolchain_bin/rustdoc" \
            "$cargo_bin" "$@"
        else
            rustup run "$toolchain" cargo "$@"
        fi
    else
        cargo "$@"
    fi
}

rust_chatbot_target_dir() {
    local repo_root="${1:-$PWD}"
    if [ -n "${CARGO_TARGET_DIR:-}" ]; then
        printf '%s\n' "$CARGO_TARGET_DIR"
    else
        printf '%s/target/ui-stack\n' "${XDG_CACHE_HOME:-$HOME/.cache}/rust-chatbot"
    fi
}

rust_chatbot_release_dir() {
    local repo_root="${1:-$PWD}"
    printf '%s/release\n' "$(rust_chatbot_target_dir "$repo_root")"
}

rust_chatbot_adapter_repo_root() {
    local desktop_repo_root="${1:-$PWD}"
    if [ -n "${RUST_CHATBOT_ADAPTER_ROOT:-}" ]; then
        printf '%s\n' "$RUST_CHATBOT_ADAPTER_ROOT"
    else
        printf '%s\n' "$(cd "$desktop_repo_root/.." && pwd)/chatbot-adapter"
    fi
}

rust_chatbot_adapter_target_dir() {
    local desktop_repo_root="${1:-$PWD}"
    local adapter_repo_root
    adapter_repo_root="$(rust_chatbot_adapter_repo_root "$desktop_repo_root")"
    if [ -n "${RUST_CHATBOT_ADAPTER_TARGET_DIR:-}" ]; then
        printf '%s\n' "$RUST_CHATBOT_ADAPTER_TARGET_DIR"
    else
        printf '%s/target/agent-adapter\n' "${XDG_CACHE_HOME:-$HOME/.cache}/rust-chatbot"
    fi
}

rust_chatbot_adapter_release_dir() {
    local desktop_repo_root="${1:-$PWD}"
    printf '%s/release\n' "$(rust_chatbot_adapter_target_dir "$desktop_repo_root")"
}

rust_chatbot_adapter_release_binary_path() {
    local desktop_repo_root="${1:-$PWD}"
    printf '%s/agent-adapter-cli\n' "$(rust_chatbot_adapter_release_dir "$desktop_repo_root")"
}

rust_chatbot_build_agent_adapter_release() {
    local desktop_repo_root="$1"
    local adapter_repo_root
    local adapter_target_dir
    adapter_repo_root="$(rust_chatbot_adapter_repo_root "$desktop_repo_root")"
    adapter_target_dir="$(rust_chatbot_adapter_target_dir "$desktop_repo_root")"

    if [ ! -f "$adapter_repo_root/Cargo.toml" ]; then
        echo "Error: chatbot-adapter repo not found at $adapter_repo_root" >&2
        echo "Set RUST_CHATBOT_ADAPTER_ROOT to the chatbot-adapter checkout before building rust-chatbot release binaries." >&2
        return 1
    fi

    CARGO_TARGET_DIR="$adapter_target_dir" rust_chatbot_repo_cargo "$adapter_repo_root" build --release \
        --manifest-path "$adapter_repo_root/Cargo.toml" \
        -p agent-adapter-cli || return $?
}

rust_chatbot_build_release() {
    local repo_root="$1"
    CARGO_TARGET_DIR="$(rust_chatbot_target_dir "$repo_root")" rust_chatbot_repo_cargo "$repo_root" build --release \
        --manifest-path "$repo_root/Cargo.toml" \
        --bin chatbot-ctl \
        --bin rust-chatbot \
        --bin rcc \
        --bin chatbot-worker || return $?

    rust_chatbot_build_agent_adapter_release "$repo_root" || return $?
}

install_release_binary() {
    local src="$1"
    local dst="$2"
    local temp_dst="$dst.rust-chatbot-install.$$"
    local mode=""

    cp "$src" "$temp_dst"

    if chmod --reference="$src" "$temp_dst" 2>/dev/null; then
        :
    else
        mode="$(stat -f '%Lp' "$src" 2>/dev/null || stat -c '%a' "$src" 2>/dev/null)"
        if [ -n "$mode" ]; then
            chmod "$mode" "$temp_dst"
        fi
    fi

    mv "$temp_dst" "$dst"
}

rust_chatbot_install_managed_wrapper() {
    local src="$1"
    local dst="$2"

    if [ ! -f "$src" ]; then
        echo "Error: managed wrapper not found at $src" >&2
        return 1
    fi

    install_release_binary "$src" "$dst"
}

rust_chatbot_install_release_binaries() {
    local repo_root="$1"
    local install_root="${2:-${CARGO_INSTALL_ROOT:-$HOME/.local}}"
    local release_dir
    local bin_dir
    release_dir="$(rust_chatbot_release_dir "$repo_root")"
    bin_dir="$install_root/bin"

    mkdir -p "$bin_dir"
    install_release_binary "$release_dir/chatbot-ctl" "$bin_dir/chatbot-ctl"
    install_release_binary "$release_dir/rust-chatbot" "$bin_dir/rust-chatbot"
    install_release_binary "$release_dir/rcc" "$bin_dir/rcc"
    install_release_binary "$release_dir/chatbot-worker" "$bin_dir/chatbot-worker"
    install_release_binary "$(rust_chatbot_adapter_release_binary_path "$repo_root")" "$bin_dir/agent-adapter-cli"
    rust_chatbot_install_managed_wrapper "$repo_root/scripts/bin/semble" "$bin_dir/semble"
}

rust_chatbot_forgecode2_repo_root() {
    if [ -n "${RUST_CHATBOT_FORGECODE2_ROOT:-}" ]; then
        printf '%s\n' "$RUST_CHATBOT_FORGECODE2_ROOT"
    else
        printf '%s\n' "$HOME/git/others/forgecode2"
    fi
}

rust_chatbot_forge_compat_binary_path() {
    local repo_root
    repo_root="$(rust_chatbot_forgecode2_repo_root)"
    printf '%s\n' "$repo_root/target/release/forge"
}

rust_chatbot_maybe_build_forge_compat_release() {
    local repo_root
    local binary_path
    local commit_epoch
    local binary_epoch

    repo_root="$(rust_chatbot_forgecode2_repo_root)"
    binary_path="$(rust_chatbot_forge_compat_binary_path)"

    if [ ! -d "$repo_root/.git" ] || [ ! -f "$repo_root/Cargo.toml" ]; then
        echo "Skipping Forge compat rebuild check: repo not found at $repo_root"
        return 0
    fi

    commit_epoch="$(git -C "$repo_root" log -1 --format='%ct' 2>/dev/null || true)"
    if [ -z "$commit_epoch" ]; then
        echo "Skipping Forge compat rebuild check: could not read latest commit for $repo_root"
        return 0
    fi

    if [ -f "$binary_path" ]; then
        binary_epoch="$(stat -c '%Y' "$binary_path" 2>/dev/null || stat -f '%m' "$binary_path" 2>/dev/null || true)"
    else
        binary_epoch=""
    fi

    if [ -n "$binary_epoch" ] && [ "$binary_epoch" -ge "$commit_epoch" ]; then
        echo "Skipping Forge compat rebuild: $binary_path is up to date"
        return 0
    fi

    rust_chatbot_build_forge_compat_release
}

rust_chatbot_build_forge_compat_release() {
    local repo_root
    local build_target_dir
    local stage_dir
    local release_dir
    repo_root="$(rust_chatbot_forgecode2_repo_root)"
    build_target_dir="${RUST_CHATBOT_FORGE_COMPAT_TARGET_DIR:-$repo_root/target}"
    stage_dir="$build_target_dir/release"
    release_dir="$repo_root/target/release"

    if [ ! -f "$repo_root/Cargo.toml" ]; then
        echo "Error: Forge compat repo not found at $repo_root" >&2
        echo "Set RUST_CHATBOT_FORGECODE2_ROOT to the forgecode2 checkout before installing GeminiCliForge support." >&2
        return 1
    fi

    CARGO_TARGET_DIR="$build_target_dir" rust_chatbot_repo_cargo "$repo_root" build --release \
        --manifest-path "$repo_root/Cargo.toml" \
        --bin rust-chatbot \
        --bin forge || return $?

    mkdir -p "$release_dir"
    if [ "$stage_dir" != "$release_dir" ]; then
        install_release_binary "$stage_dir/rust-chatbot" "$release_dir/rust-chatbot"
        install_release_binary "$stage_dir/forge" "$release_dir/forge"
    fi
}

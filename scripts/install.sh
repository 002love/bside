#!/bin/sh
# One-line installer for bside (macOS & Linux):
#   curl -fsSL https://raw.githubusercontent.com/002love/bside/main/scripts/install.sh | sh
#
# Optional environment variables:
#   BSIDE_VERSION      Release tag to install (e.g. v0.1.0). Defaults to latest release.
#   BSIDE_INSTALL_DIR  Destination directory for the binary. Defaults to "$HOME/.local/bin".

set -eu

REPO="002love/bside"
BINARY_NAME="bside"
INSTALL_DIR="${BSIDE_INSTALL_DIR:-${HOME}/.local/bin}"

say() {
    printf '%s\n' "$*"
}

err() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

need_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        err "required command not found: $1"
    fi
}

detect_os() {
    case "$(uname -s)" in
        Darwin)
            printf 'macos'
            ;;
        Linux)
            printf 'linux'
            ;;
        *)
            err "unsupported operating system: $(uname -s)"
            ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64 | amd64)
            printf 'x86_64'
            ;;
        arm64 | aarch64)
            printf 'aarch64'
            ;;
        *)
            err "unsupported CPU architecture: $(uname -m)"
            ;;
    esac
}

resolve_version() {
    if [ -n "${BSIDE_VERSION:-}" ]; then
        case "${BSIDE_VERSION}" in
            v*)
                printf '%s' "${BSIDE_VERSION}"
                ;;
            *)
                printf 'v%s' "${BSIDE_VERSION}"
                ;;
        esac
        return
    fi

    api_url="https://api.github.com/repos/${REPO}/releases/latest"
    response="$(curl -fsSL -H "Accept: application/vnd.github+json" -H "User-Agent: bside-installer" "${api_url}")" \
        || err "failed to fetch latest release metadata from ${api_url}"

    tag="$(printf '%s\n' "${response}" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
    if [ -z "${tag}" ]; then
        err "could not determine latest release tag from GitHub API response"
    fi

    printf '%s' "${tag}"
}

compute_sha256() {
    file_path="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "${file_path}" | awk '{print tolower($1)}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "${file_path}" | awk '{print tolower($1)}'
    else
        err "neither sha256sum nor shasum is available to verify checksums"
    fi
}

verify_checksum() {
    archive_path="$1"
    archive_name="$2"
    sums_path="$3"

    expected="$(awk -v target="${archive_name}" '$2 == target || $2 == ("*" target) {print tolower($1); exit}' "${sums_path}")"
    if [ -z "${expected}" ]; then
        err "checksum for ${archive_name} not found in SHA256SUMS.txt"
    fi

    actual="$(compute_sha256 "${archive_path}")"
    if [ "${actual}" != "${expected}" ]; then
        err "SHA256 checksum mismatch for ${archive_name} (expected ${expected}, got ${actual})"
    fi
}

main() {
    need_cmd curl
    need_cmd tar
    need_cmd awk
    need_cmd sed
    need_cmd mktemp

    os="$(detect_os)"
    arch="$(detect_arch)"
    tag="$(resolve_version)"

    archive_name="${BINARY_NAME}-${tag}-${arch}-${os}.tar.gz"
    base_url="https://github.com/${REPO}/releases/download/${tag}"
    archive_url="${base_url}/${archive_name}"
    sums_url="${base_url}/SHA256SUMS.txt"

    tmp_dir="$(mktemp -d)"
    trap 'rm -rf "${tmp_dir}"' EXIT INT TERM

    say "Installing ${BINARY_NAME} ${tag} (${arch}-${os})..."

    archive_path="${tmp_dir}/${archive_name}"
    sums_path="${tmp_dir}/SHA256SUMS.txt"

    curl -fsSL -o "${archive_path}" "${archive_url}" \
        || err "failed to download ${archive_url}"
    curl -fsSL -o "${sums_path}" "${sums_url}" \
        || err "failed to download ${sums_url}"

    say "Verifying SHA256 checksum..."
    verify_checksum "${archive_path}" "${archive_name}" "${sums_path}"

    tar -xzf "${archive_path}" -C "${tmp_dir}"
    if [ ! -f "${tmp_dir}/${BINARY_NAME}" ]; then
        err "expected binary '${BINARY_NAME}' not found inside ${archive_name}"
    fi

    mkdir -p "${INSTALL_DIR}"
    cp "${tmp_dir}/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
    chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

    say ""
    say "Installed ${BINARY_NAME} ${tag} to ${INSTALL_DIR}/${BINARY_NAME}"

    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*)
            ;;
        *)
            say ""
            say "Warning: ${INSTALL_DIR} is not in your PATH."
            say "Add it to your shell profile (~/.zshrc, ~/.bashrc, or ~/.profile):"
            say "  export PATH=\"${INSTALL_DIR}:\$PATH\""
            ;;
    esac

    say ""
    say "Next steps:"
    say "  bside login"
    say "  bside service install"
    say "  bside watch"
}

main "$@"

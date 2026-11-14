#!/usr/bin/env bash
#
# Build Perfboard Designer from this checkout.
#
#   ./quickstart.sh            ask what to build (defaults to the executable)
#   ./quickstart.sh exe        release executables only, run from target/release
#   ./quickstart.sh deb        Debian/Ubuntu package
#   ./quickstart.sh rpm        Fedora/openSUSE/RHEL package
#   ./quickstart.sh appimage   portable AppImage for any Linux distribution
#   ./quickstart.sh deb,rpm    any comma-separated mix of deb, rpm, appimage
#
# Packages install the desktop app as /usr/bin/perfboard-designer, the MCP
# adapter as /usr/bin/perfboard-mcp-server, and the component library as an
# application resource.

set -euo pipefail

project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

usage() {
  sed -n '3,14p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

target="${1:-}"
if [[ "$target" == "-h" || "$target" == "--help" ]]; then
  usage
  exit 0
fi

if [[ -z "$target" ]]; then
  if [[ -t 0 ]]; then
    echo "What do you want to build?"
    echo "  1) Executables only (default)"
    echo "  2) .deb package"
    echo "  3) .rpm package"
    echo "  4) AppImage"
    echo "  5) .deb, .rpm and AppImage"
    read -r -p "Choice [1]: " choice
    case "${choice:-1}" in
      1) target=exe ;;
      2) target=deb ;;
      3) target=rpm ;;
      4) target=appimage ;;
      5) target=deb,rpm,appimage ;;
      *) echo "error: unknown choice '$choice'" >&2; exit 1 ;;
    esac
  else
    target=exe
  fi
fi

if [[ "$target" != exe ]]; then
  for format in ${target//,/ }; do
    case "$format" in
      deb | rpm | appimage) ;;
      *)
        echo "error: unknown target '$format'" >&2
        usage >&2
        exit 1
        ;;
    esac
  done
fi

if [[ "$target" != exe && "$(uname -s)" != Linux ]]; then
  echo "error: .deb and .rpm packages can only be built on Linux" >&2
  exit 1
fi

for command in cargo rustc npm; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "error: required command '$command' was not found in PATH" >&2
    exit 1
  fi
done

if ! cargo tauri --version >/dev/null 2>&1; then
  echo "error: Tauri CLI 2 is required; install it with:" >&2
  echo "  cargo install tauri-cli --version '^2' --locked" >&2
  exit 1
fi

cd -- "$project_root"

echo "Installing locked frontend dependencies..."
npm --prefix frontend ci

echo "Building the MCP server..."
cargo build --release --locked -p perfboard-mcp-server
mcp_server="$project_root/target/release/perfboard-mcp-server"

if [[ "$target" == exe ]]; then
  echo "Building Perfboard Designer..."
  cargo tauri build --no-bundle

  cat <<EOF

Build complete:
  App:        $project_root/target/release/perfboard-designer
  MCP server: $mcp_server

The standard component library is built into the executable, so it can
be copied anywhere. To use the prebuilt MCP server instead of
'cargo run', register it with your client, for example:
  claude mcp add --scope user perfboard-designer -- $mcp_server
EOF
  exit 0
fi

# Tauri bundles a sidecar named with the host target triple and installs it
# without the suffix.
host_triple="$(rustc -vV | sed -n 's/^host: //p')"
mkdir -p src-tauri/binaries
cp "$mcp_server" "src-tauri/binaries/perfboard-mcp-server-$host_triple"

echo "Building Perfboard Designer packages ($target)..."
cargo tauri build --bundles "$target" --config src-tauri/tauri.bundle.conf.json

echo
echo "Build complete:"
for format in ${target//,/ }; do
  extension="$format"
  [[ "$format" == appimage ]] && extension=AppImage
  for package in target/release/bundle/"$format"/*."$extension"; do
    [[ -e "$package" ]] && echo "  $project_root/$package"
  done
done

cat <<'EOF'

After installing a package, register the MCP server with your client:
  claude mcp add --scope user perfboard-designer -- /usr/bin/perfboard-mcp-server
For Codex, see "MCP access from Codex and Claude Code" in README.md.

The desktop app must be running for MCP tools to work. Inside this checkout,
the project's .mcp.json still runs the server with Cargo instead.
EOF

if [[ ",$target," == *,appimage,* ]]; then
  echo
  echo "The AppImage carries the MCP server inside the image, not on PATH; use a"
  echo "package or './quickstart.sh exe' for MCP access."
fi

#!/bin/bash
# VBDP End-to-End Demo
# This script demonstrates the complete publisher -> server -> client workflow.
#
# The Verified Binary Distribution Protocol (VBDP) enables secure, signed
# binary distribution with delta updates. This demo walks through:
#   1. Publisher: Initialize project, register versions, sign, and publish
#   2. Server: Host the update server
#   3. Client: Register managed binaries and check for updates

set -e

#------------------------------------------------------------------------------
# Configuration
#------------------------------------------------------------------------------
DEMO_DIR="/tmp/vbdp-demo"
SERVER_PORT=8080
SERVER_HOST="127.0.0.1"
APP_NAME="demo-app"
PLATFORM="linux-x86_64"

# Path to built binaries (adjust if using a different build profile)
VBDP_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PUBLISHER_BIN="${VBDP_ROOT}/target/release/vbdp-publisher"
SERVER_BIN="${VBDP_ROOT}/target/release/vbdp-server"
CLIENT_BIN="${VBDP_ROOT}/target/release/vbdp-client"

# Use debug binaries as fallback
if [ ! -f "$PUBLISHER_BIN" ]; then
    PUBLISHER_BIN="${VBDP_ROOT}/target/debug/vbdp-publisher"
fi
if [ ! -f "$SERVER_BIN" ]; then
    SERVER_BIN="${VBDP_ROOT}/target/debug/vbdp-server"
fi
if [ ! -f "$CLIENT_BIN" ]; then
    CLIENT_BIN="${VBDP_ROOT}/target/debug/vbdp-client"
fi

#------------------------------------------------------------------------------
# Colors for output
#------------------------------------------------------------------------------
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

echo_step() { echo -e "\n${GREEN}=== $1 ===${NC}"; }
echo_info() { echo -e "${YELLOW}$1${NC}"; }
echo_cmd() { echo -e "${CYAN}> $1${NC}"; }
echo_error() { echo -e "${RED}ERROR: $1${NC}"; }

#------------------------------------------------------------------------------
# Cleanup function - runs on exit
#------------------------------------------------------------------------------
SERVER_PID=""

cleanup() {
    echo_step "Cleaning up..."

    # Kill server if running
    if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
        echo_info "Stopping server (PID: $SERVER_PID)"
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi

    # Remove demo directory
    if [ -d "$DEMO_DIR" ]; then
        echo_info "Removing demo directory: $DEMO_DIR"
        rm -rf "$DEMO_DIR"
    fi

    echo_info "Cleanup complete."
}

trap cleanup EXIT

#------------------------------------------------------------------------------
# Prerequisite checks
#------------------------------------------------------------------------------
echo_step "Checking prerequisites"

check_binary() {
    local name=$1
    local path=$2
    if [ -f "$path" ]; then
        echo_info "Found $name: $path"
        return 0
    else
        echo_error "$name not found at $path"
        echo_info "Run 'cargo build --release' from the project root first."
        return 1
    fi
}

check_binary "vbdp-publisher" "$PUBLISHER_BIN" || exit 1
check_binary "vbdp-server" "$SERVER_BIN" || exit 1
check_binary "vbdp-client" "$CLIENT_BIN" || exit 1

# Check for required tools
command -v curl >/dev/null 2>&1 || { echo_error "curl is required but not installed."; exit 1; }

#------------------------------------------------------------------------------
# Step 1: Setup demo environment
#------------------------------------------------------------------------------
echo_step "Step 1: Setting up demo environment"

echo_info "Creating directory structure at $DEMO_DIR"
mkdir -p "$DEMO_DIR"/{publisher,server-data,client}

# Create publisher workspace
PUBLISHER_DIR="$DEMO_DIR/publisher"
cd "$PUBLISHER_DIR"

echo_info "Demo directories created:"
echo "  - $DEMO_DIR/publisher    (publisher workspace)"
echo "  - $DEMO_DIR/server-data  (server storage)"
echo "  - $DEMO_DIR/client       (client data)"

#------------------------------------------------------------------------------
# Step 2: Create sample application binaries
#------------------------------------------------------------------------------
echo_step "Step 2: Creating sample application binaries"

# For this demo, we create simple text files to simulate binary releases.
# In a real scenario, these would be compiled executables.

echo_info "Creating v1.0.0 binary..."
cat > "$DEMO_DIR/app-v1.0.0" << 'EOF'
#!/bin/bash
# Demo App v1.0.0
echo "Demo Application Version 1.0.0"
echo "This is the initial release."
EOF
chmod +x "$DEMO_DIR/app-v1.0.0"

echo_info "Creating v1.1.0 binary (with new features)..."
cat > "$DEMO_DIR/app-v1.1.0" << 'EOF'
#!/bin/bash
# Demo App v1.1.0
echo "Demo Application Version 1.1.0"
echo "This is the initial release."
echo "New feature: Enhanced performance!"
echo "New feature: Bug fixes and improvements!"
EOF
chmod +x "$DEMO_DIR/app-v1.1.0"

echo_info "Sample binaries created:"
ls -la "$DEMO_DIR"/app-v*

#------------------------------------------------------------------------------
# Step 3: Initialize VBDP publisher project
#------------------------------------------------------------------------------
echo_step "Step 3: Initializing VBDP publisher project"

cd "$PUBLISHER_DIR"

echo_info "The publisher init command creates:"
echo "  - .vbdp/ directory structure"
echo "  - Ed25519 signing keypair"
echo "  - Publisher database"

# Initialize with empty passphrase for demo (non-interactive)
# In production, use a strong passphrase!
echo_cmd "vbdp-publisher init"
echo "" | "$PUBLISHER_BIN" init 2>&1 || {
    # If init prompts for passphrase, try piping empty strings
    echo -e "\n" | "$PUBLISHER_BIN" init 2>&1 || true
}

echo_info "Project initialized. Keys generated in .vbdp/keys/"
ls -la "$PUBLISHER_DIR/.vbdp/keys/" 2>/dev/null || echo_info "(keys directory)"

# Copy public key for client use later
PUBLIC_KEY_FILE="$PUBLISHER_DIR/.vbdp/keys/public.key"

#------------------------------------------------------------------------------
# Step 4: Register version 1.0.0
#------------------------------------------------------------------------------
echo_step "Step 4: Registering version 1.0.0"

echo_info "Registering a version:"
echo "  - Computes file hashes (Blake3, SHA-256)"
echo "  - Stores version metadata in database"
echo "  - Optionally generates diffs from previous versions"

echo_cmd "vbdp-publisher register --name $APP_NAME --version 1.0.0 --file $DEMO_DIR/app-v1.0.0 --platform $PLATFORM"
"$PUBLISHER_BIN" register \
    --name "$APP_NAME" \
    --version "1.0.0" \
    --file "$DEMO_DIR/app-v1.0.0" \
    --platform "$PLATFORM"

#------------------------------------------------------------------------------
# Step 5: Register version 1.1.0 (auto-generates diff)
#------------------------------------------------------------------------------
echo_step "Step 5: Registering version 1.1.0 (auto-generates diff)"

echo_info "When registering a new version, VBDP automatically generates"
echo_info "binary diffs from previous versions for efficient delta updates."

echo_cmd "vbdp-publisher register --name $APP_NAME --version 1.1.0 --file $DEMO_DIR/app-v1.1.0 --platform $PLATFORM"
"$PUBLISHER_BIN" register \
    --name "$APP_NAME" \
    --version "1.1.0" \
    --file "$DEMO_DIR/app-v1.1.0" \
    --platform "$PLATFORM"

#------------------------------------------------------------------------------
# Step 6: List registered versions
#------------------------------------------------------------------------------
echo_step "Step 6: Listing registered versions"

echo_cmd "vbdp-publisher list"
"$PUBLISHER_BIN" list

#------------------------------------------------------------------------------
# Step 7: Sign versions
#------------------------------------------------------------------------------
echo_step "Step 7: Signing versions with Ed25519 key"

echo_info "Signing creates a cryptographic signature of the version manifest."
echo_info "Clients verify this signature before applying updates."

# Sign v1.0.0 (with empty passphrase for demo)
echo_cmd "vbdp-publisher sign --name $APP_NAME --version 1.0.0"
echo "" | "$PUBLISHER_BIN" sign --name "$APP_NAME" --version "1.0.0" 2>&1 || {
    echo -e "\n" | "$PUBLISHER_BIN" sign --name "$APP_NAME" --version "1.0.0" 2>&1 || true
}

# Sign v1.1.0
echo_cmd "vbdp-publisher sign --name $APP_NAME --version 1.1.0"
echo "" | "$PUBLISHER_BIN" sign --name "$APP_NAME" --version "1.1.0" 2>&1 || {
    echo -e "\n" | "$PUBLISHER_BIN" sign --name "$APP_NAME" --version "1.1.0" 2>&1 || true
}

#------------------------------------------------------------------------------
# Step 8: Start the update server
#------------------------------------------------------------------------------
echo_step "Step 8: Starting VBDP update server"

echo_info "The server provides REST API endpoints for:"
echo "  - /health - Health check"
echo "  - /api/v1/apps/{app}/check-update - Check for updates"
echo "  - /api/v1/publish - Publish new versions"
echo "  - Binary and diff downloads"

echo_cmd "vbdp-server --host $SERVER_HOST --port $SERVER_PORT --data-dir $DEMO_DIR/server-data &"
"$SERVER_BIN" \
    --host "$SERVER_HOST" \
    --port "$SERVER_PORT" \
    --data-dir "$DEMO_DIR/server-data" &
SERVER_PID=$!

echo_info "Server started with PID: $SERVER_PID"

# Wait for server to be ready
echo_info "Waiting for server to be ready..."
sleep 2

# Check server health
for i in {1..10}; do
    if curl -s "http://${SERVER_HOST}:${SERVER_PORT}/health" > /dev/null 2>&1; then
        echo_info "Server is ready!"
        break
    fi
    if [ $i -eq 10 ]; then
        echo_error "Server failed to start within timeout"
        exit 1
    fi
    sleep 1
done

# Show health response
echo_cmd "curl http://${SERVER_HOST}:${SERVER_PORT}/health"
curl -s "http://${SERVER_HOST}:${SERVER_PORT}/health" | python3 -m json.tool 2>/dev/null || \
    curl -s "http://${SERVER_HOST}:${SERVER_PORT}/health"

#------------------------------------------------------------------------------
# Step 9: Publish versions to server
#------------------------------------------------------------------------------
echo_step "Step 9: Publishing versions to server"

echo_info "Publishing uploads:"
echo "  - Binary files"
echo "  - Signatures"
echo "  - Delta diffs (if available)"

SERVER_URL="http://${SERVER_HOST}:${SERVER_PORT}"

echo_cmd "vbdp-publisher publish --name $APP_NAME --version 1.0.0 --server-url $SERVER_URL"
"$PUBLISHER_BIN" publish \
    --name "$APP_NAME" \
    --version "1.0.0" \
    --server-url "$SERVER_URL"

echo ""
echo_cmd "vbdp-publisher publish --name $APP_NAME --version 1.1.0 --server-url $SERVER_URL"
"$PUBLISHER_BIN" publish \
    --name "$APP_NAME" \
    --version "1.1.0" \
    --server-url "$SERVER_URL"

#------------------------------------------------------------------------------
# Step 10: Setup client with v1.0.0
#------------------------------------------------------------------------------
echo_step "Step 10: Setting up client with v1.0.0 installed"

CLIENT_DIR="$DEMO_DIR/client"
CLIENT_CONFIG="$CLIENT_DIR/client.toml"
INSTALLED_BINARY="$CLIENT_DIR/demo-app"

# Copy v1.0.0 as the "installed" version
cp "$DEMO_DIR/app-v1.0.0" "$INSTALLED_BINARY"
chmod +x "$INSTALLED_BINARY"

echo_info "Installed binary version:"
"$INSTALLED_BINARY" || head -5 "$INSTALLED_BINARY"

# Create client config
echo_info "Creating client configuration..."
cat > "$CLIENT_CONFIG" << EOF
# VBDP Client Configuration
server_url = "$SERVER_URL"
check_interval_secs = 3600
verify_signatures = true
data_dir = "$CLIENT_DIR/data"
EOF

echo_info "Client config:"
cat "$CLIENT_CONFIG"

# Ensure public key exists, create a copy in client dir
if [ -f "$PUBLIC_KEY_FILE" ]; then
    cp "$PUBLIC_KEY_FILE" "$CLIENT_DIR/publisher.pub"
    echo_info "Copied publisher public key to client directory"
else
    echo_error "Public key not found at $PUBLIC_KEY_FILE"
    echo_info "Skipping client registration (key required for signature verification)"
fi

#------------------------------------------------------------------------------
# Step 11: Register binary with client
#------------------------------------------------------------------------------
echo_step "Step 11: Registering binary with client"

echo_info "The client tracks managed binaries and their publishers' public keys."
echo_info "This enables automatic signature verification on updates."

if [ -f "$CLIENT_DIR/publisher.pub" ]; then
    echo_cmd "vbdp-client --config $CLIENT_CONFIG add --name $APP_NAME --path $INSTALLED_BINARY --public-key-file $CLIENT_DIR/publisher.pub"
    "$CLIENT_BIN" \
        --config "$CLIENT_CONFIG" \
        add \
        --name "$APP_NAME" \
        --path "$INSTALLED_BINARY" \
        --public-key-file "$CLIENT_DIR/publisher.pub" 2>&1 || true
else
    echo_info "Skipping client add (no public key available)"
fi

#------------------------------------------------------------------------------
# Step 12: List managed binaries
#------------------------------------------------------------------------------
echo_step "Step 12: Listing managed binaries"

echo_cmd "vbdp-client --config $CLIENT_CONFIG list"
"$CLIENT_BIN" --config "$CLIENT_CONFIG" list 2>&1 || true

#------------------------------------------------------------------------------
# Step 13: Check for updates
#------------------------------------------------------------------------------
echo_step "Step 13: Checking for updates"

echo_info "The client checks the server for available updates."
echo_info "Current version: 1.0.0"
echo_info "Latest version: 1.1.0"

echo_cmd "vbdp-client --config $CLIENT_CONFIG --check-now"
"$CLIENT_BIN" --config "$CLIENT_CONFIG" --check-now 2>&1 || true

#------------------------------------------------------------------------------
# Step 14: Show update status
#------------------------------------------------------------------------------
echo_step "Step 14: Showing update status"

echo_cmd "vbdp-client --config $CLIENT_CONFIG status"
"$CLIENT_BIN" --config "$CLIENT_CONFIG" status 2>&1 || true

#------------------------------------------------------------------------------
# Step 15: Direct API test (check-update endpoint)
#------------------------------------------------------------------------------
echo_step "Step 15: Testing update API directly"

echo_info "Querying the server's check-update endpoint directly..."

echo_cmd "curl '$SERVER_URL/api/v1/apps/$APP_NAME/check-update?current_version=1.0.0&platform=$PLATFORM'"
RESPONSE=$(curl -s "$SERVER_URL/api/v1/apps/$APP_NAME/check-update?current_version=1.0.0&platform=$PLATFORM" 2>&1)
echo "$RESPONSE" | python3 -m json.tool 2>/dev/null || echo "$RESPONSE"

#------------------------------------------------------------------------------
# Demo Summary
#------------------------------------------------------------------------------
echo_step "Demo Complete!"

echo ""
echo -e "${GREEN}Summary of the VBDP workflow:${NC}"
echo ""
echo "  1. PUBLISHER SIDE:"
echo "     - Initialize project with 'vbdp-publisher init'"
echo "     - Register versions with 'vbdp-publisher register'"
echo "     - Sign versions with 'vbdp-publisher sign'"
echo "     - Publish to server with 'vbdp-publisher publish'"
echo ""
echo "  2. SERVER SIDE:"
echo "     - Run 'vbdp-server' to host update infrastructure"
echo "     - Serves binaries, diffs, and signatures via REST API"
echo ""
echo "  3. CLIENT SIDE:"
echo "     - Add binaries to manage with 'vbdp-client add'"
echo "     - Check for updates with 'vbdp-client --check-now'"
echo "     - Run as daemon with 'vbdp-client --daemon'"
echo ""
echo -e "${YELLOW}Key files created during this demo:${NC}"
echo "  - Publisher keys: $PUBLISHER_DIR/.vbdp/keys/"
echo "  - Publisher DB:   $PUBLISHER_DIR/.vbdp/publisher.db"
echo "  - Server data:    $DEMO_DIR/server-data/"
echo "  - Client config:  $CLIENT_CONFIG"
echo ""
echo -e "${CYAN}API Endpoints:${NC}"
echo "  - Health:       GET  /health"
echo "  - Check Update: GET  /api/v1/apps/{app}/check-update?current_version=X&platform=Y"
echo "  - Publish:      POST /api/v1/publish"
echo ""
echo -e "${GREEN}The demo environment will be cleaned up automatically.${NC}"
echo "Press Ctrl+C to exit, or wait 10 seconds..."

# Keep server running briefly for manual testing
sleep 10

#!/bin/bash

echo "Testing Claude CLI integration..."

# Test if claude command exists
if ! command -v claude &> /dev/null; then
    echo "❌ Claude CLI not found"
    exit 1
fi

echo "✅ Claude CLI found at: $(which claude)"

# Test claude help
echo ""
echo "Testing claude --help..."
if claude --help > /dev/null 2>&1; then
    echo "✅ Claude help command works"
else
    echo "❌ Claude help command failed"
fi

# Test claude version
echo ""
echo "Testing claude version..."
if claude --version; then
    echo "✅ Claude version command works"
else
    echo "❌ Claude version command failed"
fi

# Test claude status or config
echo ""
echo "Testing Claude configuration..."
if claude config > /dev/null 2>&1; then
    echo "✅ Claude config accessible"
else
    echo "⚠️  Claude may need configuration. Run: claude setup-token"
fi

echo ""
echo "If you see authentication errors when running the app, please run:"
echo "  claude setup-token"
echo "And follow the instructions to set up your API key."

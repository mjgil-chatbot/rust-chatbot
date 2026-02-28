#!/bin/bash

echo "Testing Claude CLI command construction with --dangerously-skip-permissions..."

# Test the actual command that should be generated
echo ""
echo "Testing system prompt command construction:"
echo "Expected: claude --append-system-prompt \"you are a chatbot that always responds in json\" --dangerously-skip-permissions -p \"what day is it?\""

echo ""
echo "Testing without system prompt:"
echo "Expected: claude --dangerously-skip-permissions -p \"what day is it?\""

echo ""
echo "Now let's test if claude CLI accepts these commands..."

# Test without system prompt first
echo "1. Testing basic command with --dangerously-skip-permissions:"
if timeout 5s claude --dangerously-skip-permissions -p "hello" >/dev/null 2>&1; then
    echo "✅ Basic claude command with --dangerously-skip-permissions works"
else
    echo "❌ Basic claude command failed - this suggests authentication issues"
    echo "Please run: claude setup-token"
fi

echo ""
echo "2. Testing with system prompt and --dangerously-skip-permissions:"
if timeout 5s claude --append-system-prompt "test" --dangerously-skip-permissions -p "hello" >/dev/null 2>&1; then
    echo "✅ System prompt command with --dangerously-skip-permissions works"
else
    echo "❌ System prompt command failed"
    echo "This could be due to:"
    echo "  - Authentication issues (run: claude setup-token)"
    echo "  - Unsupported Claude CLI version"
    echo "  - Network connectivity"
fi

echo ""
echo "Note: The --dangerously-skip-permissions flag prevents Claude from asking"
echo "permission for every action, making the desktop app experience much smoother."
echo ""
echo "The application will now show commands like:"
echo "Command: claude --append-system-prompt \"prompt\" --dangerously-skip-permissions -p \"query\""

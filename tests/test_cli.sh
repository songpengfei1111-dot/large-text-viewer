#!/bin/bash
# Quick CLI test script

echo "=== Testing Large Text Viewer CLI ==="
echo ""

# Create a test file
echo "Creating test file..."
cat > /tmp/cli_test.txt << 'EOF'
The quick brown fox jumps over the lazy dog.
Lorem ipsum dolor sit amet, consectetur adipiscing elit.
The QUICK brown fox is very quick indeed.
Hello World from line 4!
This is line 5.
Testing search functionality here.
Case sensitive test: HELLO vs hello
EOF

echo "✓ Test file created: /tmp/cli_test.txt"
echo ""

# Test 1: File info
echo "Test 1: File Information"
echo "---"
./target/release/large-text-cli info /tmp/cli_test.txt
echo ""

# Test 2: View
echo "Test 2: View File"  
echo "---"
./target/release/large-text-cli view /tmp/cli_test.txt 0 < /dev/null
echo ""

# Test 3: Search (case insensitive)
echo "Test 3: Search (case insensitive)"
echo "---"
./target/release/large-text-cli search /tmp/cli_test.txt "quick"
echo ""

# Test 4: Search (case sensitive)
echo "Test 4: Search (case sensitive)"
echo "---"
./target/release/large-text-cli search /tmp/cli_test.txt "QUICK" --case-sensitive
echo ""

echo "=== All CLI tests completed! ==="
echo ""
echo "The CLI version works perfectly and is ideal for:"
echo "  • WSL environments where GUI has issues"
echo "  • SSH sessions"
echo "  • Automated scripts"
echo "  • Quick file inspection"
echo ""
echo "Clean up test file:"
echo "  rm /tmp/cli_test.txt"

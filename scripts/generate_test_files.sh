#!/usr/bin/env bash
# Script to generate test files of various sizes for the large text file viewer

echo "Generating test files..."

# Create test directory
mkdir -p test_files

# Generate small test file (100 lines)
echo "Creating small.txt (100 lines)..."
for i in {1..100}; do
    echo "Line $i: This is a test line with some sample text for searching and replacing." >> test_files/small.txt
done

# Generate medium test file (10,000 lines)
echo "Creating medium.txt (10,000 lines)..."
for i in {1..10000}; do
    echo "Line $i: Lorem ipsum dolor sit amet, consectetur adipiscing elit. Pattern_$((i % 10))" >> test_files/medium.txt
done

# Generate large test file (100,000 lines)
echo "Creating large.txt (100,000 lines)..."
for i in {1..100000}; do
    echo "Line $i: The quick brown fox jumps over the lazy dog. ID=$i STATUS=active" >> test_files/large.txt
done

# Generate file with regex patterns for testing
echo "Creating regex_test.txt..."
cat > test_files/regex_test.txt << 'EOF'
test123 - This line contains digits
test456 - Another line with digits
testABC - This line has no digits
email1@example.com - An email address
email2@domain.org - Another email
phone: 123-456-7890
phone: 987-654-3210
URL: https://example.com/path
URL: http://domain.org/page?id=123
Normal line without special patterns
EOF

echo "Test files created in test_files directory:"
ls -lh test_files/